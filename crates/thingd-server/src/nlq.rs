use crate::config::NlqConfig;
use crate::engine::EnginePool;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NlqIntent {
    pub action: String,
    pub collection: String,
    pub function: Option<String>,
    pub field: Option<String>,
    pub group_by: Option<String>,
    pub bucket: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NlqResult {
    pub answer: String,
    pub data: Value,
    pub intent: NlqIntent,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

fn build_prompt(schemas_json: &str) -> String {
    format!(
        r#"You are a data analysis assistant. The user has a thingd database with these collections and inferred schemas:

{schemas_json}

You can perform these operations on the data:
- "aggregate": count/sum/avg/min/max with optional groupBy
- "timeseries": time-bucketed aggregation by hour/day/week/month
- "search": full-text search across objects

Respond with ONLY a JSON object (no markdown, no explanation) matching this type:
{{
  "action": "aggregate" | "timeseries" | "search",
  "collection": "string (collection name)",
  "function": "count" | "sum" | "avg" | "min" | "max" (omit for search)",
  "field": "string (field name for sum/avg/min/max, omit for count)",
  "groupBy": "string (field name to group by, optional)",
  "bucket": "hour" | "day" | "week" | "month" (only for timeseries)",
  "query": "string (search query, only for search action)",
  "limit": number (optional, max 100)
}}

Example: {{ "action": "aggregate", "collection": "orders", "function": "sum", "field": "revenue", "groupBy": "region" }}
Example: {{ "action": "timeseries", "collection": "sales", "function": "count", "bucket": "month" }}
Example: {{ "action": "search", "collection": "notes", "query": "meeting notes" }}"#
    )
}

async fn call_llm(
    config: &NlqConfig,
    system_prompt: &str,
    user_message: &str,
) -> Result<String, String> {
    let url = format!("{}/chat/completions", config.endpoint.trim_end_matches('/'));
    let client = reqwest::Client::new();

    let mut body = json!({
        "model": config.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_message }
        ],
        "max_tokens": config.max_tokens,
        "temperature": 0.1,
    });

    if !config.api_key.is_empty() {
        body["api_key"] = json!(config.api_key);
    }

    let req = client.post(&url).json(&body);

    let req = if !config.api_key.is_empty() {
        req.header("Authorization", format!("Bearer {}", config.api_key))
    } else {
        req
    };

    let resp = req.send().await.map_err(|e| format!("LLM request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("LLM response read failed: {e}"))?;

    if !status.is_success() {
        return Err(format!("LLM returned {status}: {text}"));
    }

    let chat: ChatResponse =
        serde_json::from_str(&text).map_err(|e| format!("LLM response parse failed: {e} — body: {text}"))?;

    chat.choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| "LLM returned no choices".to_string())
}

fn parse_intent(text: &str) -> Result<NlqIntent, String> {
    let cleaned = text
        .trim()
        .strip_prefix("```json")
        .or_else(|| text.trim().strip_prefix("```"))
        .map(|s| s.trim_end_matches("```").trim())
        .unwrap_or(text.trim());

    serde_json::from_str::<NlqIntent>(cleaned)
        .map_err(|e| format!("Failed to parse LLM response as intent: {e} — raw: {cleaned}"))
}

/// Execute an NLQ query: schema reflection → LLM call → intent → execution → result.
pub async fn execute_nlq(
    pool: &EnginePool,
    config: &NlqConfig,
    question: &str,
    collection_filter: Option<&str>,
) -> Result<NlqResult, String> {
    let e = pool.get_reader("");
    let g = e.lock();

    let schemas = g
        .schema(collection_filter, &thingd::SchemaOptions {
            sample_size: Some(config.sample_size),
        })
        .map_err(|e| format!("Schema reflection failed: {e}"))?;

    drop(g);

    if schemas.is_empty() {
        return Err("No collections found. Add objects first or specify a valid collection.".to_string());
    }

    let schemas_json =
        serde_json::to_string(&schemas).map_err(|e| format!("Schema serialization failed: {e}"))?;

    let system_prompt = build_prompt(&schemas_json);
    let llm_response = call_llm(config, &system_prompt, question).await?;

    let intent = parse_intent(&llm_response)?;

    let e = pool.get_reader("");
    let g = e.lock();

    let data: Value = match intent.action.as_str() {
        "aggregate" => {
            let fn_str = intent.function.as_deref().unwrap_or("count");
            let function = match fn_str {
                "sum" => thingd::AggregateFunction::Sum,
                "avg" => thingd::AggregateFunction::Avg,
                "min" => thingd::AggregateFunction::Min,
                "max" => thingd::AggregateFunction::Max,
                _ => thingd::AggregateFunction::Count,
            };
            let opts = thingd::AggregateOptions {
                function,
                field: intent.field.clone(),
                group_by: intent.group_by.clone(),
                filter: Vec::new(),
            };
            let result = g
                .aggregate(&intent.collection, &opts)
                .map_err(|e| format!("Aggregate failed: {e}"))?;
            serde_json::to_value(&result).unwrap_or_default()
        },
        "timeseries" => {
            let fn_str = intent.function.as_deref().unwrap_or("count");
            let function = match fn_str {
                "sum" => thingd::AggregateFunction::Sum,
                "avg" => thingd::AggregateFunction::Avg,
                "min" => thingd::AggregateFunction::Min,
                "max" => thingd::AggregateFunction::Max,
                _ => thingd::AggregateFunction::Count,
            };
            let bucket = match intent.bucket.as_deref() {
                Some("hour") => thingd::TimeBucket::Hour,
                Some("week") => thingd::TimeBucket::Week,
                Some("month") => thingd::TimeBucket::Month,
                _ => thingd::TimeBucket::Day,
            };
            let opts = thingd::TimeSeriesOptions {
                function,
                bucket,
                field: intent.field.clone(),
                filter: Vec::new(),
                from: None,
                to: None,
            };
            let result = g
                .timeseries(&intent.collection, &opts)
                .map_err(|e| format!("Timeseries failed: {e}"))?;
            serde_json::to_value(&result).unwrap_or_default()
        },
        "search" => {
            let query = intent.query.as_deref().unwrap_or(question);
            let opts = thingd::SearchOptions {
                collections: Some(vec![intent.collection.clone()]),
                limit: intent.limit,
                filter: None,
            };
            let result = g
                .search(query, opts)
                .map_err(|e| format!("Search failed: {e}"))?;
            serde_json::to_value(&result).unwrap_or_default()
        },
        other => return Err(format!("Unknown action: {other}")),
    };

    drop(g);

    let answer = if config.format_result {
        format_intent_result(&intent, &data)
    } else {
        "Query executed. See data for results.".to_string()
    };

    Ok(NlqResult {
        answer,
        data,
        intent,
    })
}

fn format_intent_result(intent: &NlqIntent, data: &Value) -> String {
    match intent.action.as_str() {
        "aggregate" => {
            let fn_name = intent.function.as_deref().unwrap_or("count");
            let total = data["total"].as_f64().unwrap_or(0.0);
            let groups = data["groups"].as_array().map(|a| a.len()).unwrap_or(0);
            if groups > 0 {
                format!("{fn_name} of {} = {total}, grouped by {} into {groups} groups",
                    intent.field.as_deref().unwrap_or("objects"),
                    intent.group_by.as_deref().unwrap_or("field"))
            } else {
                format!("{fn_name} of {} = {total}",
                    intent.field.as_deref().unwrap_or("objects"))
            }
        },
        "timeseries" => {
            let buckets = data["buckets"].as_array().map(|a| a.len()).unwrap_or(0);
            format!("Time series with {buckets} buckets")
        },
        "search" => {
            let hits = data.as_array().map(|a| a.len()).unwrap_or(0);
            format!("Found {hits} results")
        },
        _ => "Query executed.".to_string(),
    }
}
