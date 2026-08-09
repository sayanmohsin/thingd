//! Parser and canonical model for `schema.thingd` files.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

use std::collections::BTreeSet;

use pest::Parser;
use pest::iterators::Pair;
use pest_derive::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[grammar = "src/schema.pest"]
#[allow(missing_docs)]
struct ThingdSchemaParser;

/// A parsed thingd schema document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    /// Schema language version.
    pub version: u32,
    /// Optional project name.
    pub project: Option<String>,
    /// Declared collections.
    pub collections: Vec<Collection>,
    /// Declared graph links.
    pub links: Vec<Link>,
}

/// A collection declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    /// Collection name.
    pub name: String,
    /// Fields in declaration order.
    pub fields: Vec<Field>,
}

/// A collection field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Field type.
    pub field_type: FieldType,
    /// Whether the field accepts null values.
    pub optional: bool,
    /// Field annotations.
    pub annotations: Vec<Annotation>,
    /// Optional default expression.
    pub default: Option<String>,
}

/// A supported schema field type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum FieldType {
    /// UTF-8 text.
    String,
    /// Numeric value.
    Number,
    /// Boolean value.
    Boolean,
    /// RFC 3339-compatible timestamp value.
    Datetime,
    /// Arbitrary JSON value.
    Json,
    /// Fixed-width vector.
    Vector(u32),
    /// A named enum represented by its allowed string values.
    Enum(Vec<String>),
    /// A list of another scalar type.
    Array(String),
    /// An extensible named type.
    Named(String),
}

/// A field annotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Annotation {
    /// Annotation name without the `@` prefix.
    pub name: String,
    /// Optional positional arguments.
    pub arguments: Vec<String>,
}

/// A graph link declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// Link name.
    pub name: String,
    /// Source collection.
    pub from: String,
    /// Target collection.
    pub to: String,
    /// Stored link type.
    pub link_type: String,
    /// Link cardinality.
    pub cardinality: String,
}

/// Errors returned while parsing or validating a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// The source did not match the schema grammar.
    Parse(String),
    /// A schema declaration is invalid.
    Validation(String),
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "schema parse error: {message}"),
            Self::Validation(message) => write!(formatter, "schema validation error: {message}"),
        }
    }
}

impl std::error::Error for SchemaError {}

impl Schema {
    /// Return deterministic JSON suitable for storage, comparison, and hashing.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if the canonical model cannot be encoded.
    pub fn canonical_json(&self) -> Result<String, serde_json::Error> {
        let mut canonical = self.clone();
        canonical
            .collections
            .sort_by(|left, right| left.name.cmp(&right.name));
        canonical
            .links
            .sort_by(|left, right| left.name.cmp(&right.name));
        for collection in &mut canonical.collections {
            collection
                .fields
                .sort_by(|left, right| left.name.cmp(&right.name));
            for field in &mut collection.fields {
                field
                    .annotations
                    .sort_by(|left, right| left.name.cmp(&right.name));
            }
        }
        serde_json::to_string(&canonical)
    }

    /// Return the SHA-256 hash of the canonical schema representation.
    ///
    /// # Errors
    ///
    /// Returns a serialization error if canonical JSON cannot be produced.
    pub fn hash(&self) -> Result<String, serde_json::Error> {
        let mut digest = Sha256::new();
        digest.update(self.canonical_json()?.as_bytes());
        Ok(format!("sha256:{:x}", digest.finalize()))
    }

    fn validate(&self) -> Result<(), SchemaError> {
        let mut collection_names = BTreeSet::new();
        for collection in &self.collections {
            if !collection_names.insert(&collection.name) {
                return Err(SchemaError::Validation(format!(
                    "duplicate collection `{}`",
                    collection.name
                )));
            }
            let mut field_names = BTreeSet::new();
            for field in &collection.fields {
                if !field_names.insert(&field.name) {
                    return Err(SchemaError::Validation(format!(
                        "duplicate field `{}.{}`",
                        collection.name, field.name
                    )));
                }
                if let FieldType::Vector(dimensions) = field.field_type
                    && dimensions == 0
                {
                    return Err(SchemaError::Validation(format!(
                        "vector field `{}.{}` must have positive dimensions",
                        collection.name, field.name
                    )));
                }
            }
        }
        for link in &self.links {
            if !collection_names.contains(&link.from) || !collection_names.contains(&link.to) {
                return Err(SchemaError::Validation(format!(
                    "link `{}` references an unknown collection",
                    link.name
                )));
            }
        }
        Ok(())
    }
}

/// Parse and validate a `schema.thingd` source document.
///
/// # Errors
///
/// Returns a parse or validation error when the source is invalid.
pub fn parse(source: &str) -> Result<Schema, SchemaError> {
    let mut pairs = ThingdSchemaParser::parse(Rule::file, source)
        .map_err(|error| SchemaError::Parse(error.to_string()))?;
    let file = pairs
        .next()
        .ok_or_else(|| SchemaError::Parse("empty schema".into()))?;
    let mut inner = file.into_inner();
    let version_pair = inner
        .next()
        .ok_or_else(|| SchemaError::Parse("missing version declaration".into()))?;
    let version = version_pair
        .into_inner()
        .find(|pair| pair.as_rule() == Rule::integer)
        .and_then(|pair| pair.as_str().parse().ok())
        .ok_or_else(|| SchemaError::Parse("invalid schema version".into()))?;
    let mut project = None;
    let mut collections = Vec::new();
    let mut links = Vec::new();
    for pair in inner {
        match pair.as_rule() {
            Rule::project_decl => {
                let value = pair
                    .into_inner()
                    .find(|child| child.as_rule() == Rule::string)
                    .ok_or_else(|| SchemaError::Parse("invalid project declaration".into()))?;
                project = Some(parse_string(value.as_str())?);
            },
            Rule::collection => collections.push(parse_collection(pair)?),
            Rule::link => links.push(parse_link(pair)?),
            _ => {},
        }
    }
    let schema = Schema {
        version,
        project,
        collections,
        links,
    };
    schema.validate()?;
    Ok(schema)
}

fn parse_collection(pair: Pair<'_, Rule>) -> Result<Collection, SchemaError> {
    let mut inner = pair.into_inner();
    let name = inner
        .next()
        .ok_or_else(|| SchemaError::Parse("collection is missing a name".into()))?
        .as_str()
        .to_owned();
    let fields = inner
        .filter(|pair| pair.as_rule() == Rule::field)
        .map(parse_field)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Collection { name, fields })
}

fn parse_field(pair: Pair<'_, Rule>) -> Result<Field, SchemaError> {
    let mut inner = pair.into_inner();
    let name = inner.next().expect("field name").as_str().to_owned();
    let type_pair = inner
        .next()
        .ok_or_else(|| SchemaError::Parse(format!("field `{name}` is missing a type")))?;
    let field_type = parse_type(type_pair)?;
    let mut optional = false;
    let mut annotations = Vec::new();
    let mut default = None;
    for pair in inner {
        match pair.as_rule() {
            Rule::optional => optional = true,
            Rule::annotation => annotations.push(parse_annotation(pair)?),
            Rule::default_value => {
                default = pair
                    .into_inner()
                    .next()
                    .map(|value| parse_value(value.as_str()))
                    .transpose()?;
            },
            _ => {},
        }
    }
    Ok(Field {
        name,
        field_type,
        optional,
        annotations,
        default,
    })
}

fn parse_type(pair: Pair<'_, Rule>) -> Result<FieldType, SchemaError> {
    match pair.as_rule() {
        Rule::vector_type => {
            let dimensions = pair
                .into_inner()
                .next()
                .and_then(|value| value.as_str().parse().ok())
                .ok_or_else(|| SchemaError::Parse("invalid vector dimensions".into()))?;
            Ok(FieldType::Vector(dimensions))
        },
        Rule::enum_type => {
            let values = pair
                .into_inner()
                .filter(|value| value.as_rule() == Rule::string)
                .map(|value| parse_string(value.as_str()))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FieldType::Enum(values))
        },
        Rule::array_type => {
            let value = pair
                .into_inner()
                .next()
                .ok_or_else(|| SchemaError::Parse("invalid array type".into()))?;
            Ok(FieldType::Array(value.as_str().to_owned()))
        },
        Rule::ident => match pair.as_str() {
            "string" => Ok(FieldType::String),
            "number" => Ok(FieldType::Number),
            "boolean" => Ok(FieldType::Boolean),
            "datetime" => Ok(FieldType::Datetime),
            "json" => Ok(FieldType::Json),
            value => Ok(FieldType::Named(value.to_owned())),
        },
        _ => Err(SchemaError::Parse(format!(
            "unsupported field type `{}`",
            pair.as_str()
        ))),
    }
}

fn parse_annotation(pair: Pair<'_, Rule>) -> Result<Annotation, SchemaError> {
    let mut inner = pair.into_inner();
    let name = inner.next().expect("annotation name").as_str().to_owned();
    let arguments = inner
        .flat_map(Pair::into_inner)
        .map(|value| parse_value(value.as_str()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Annotation { name, arguments })
}

fn parse_link(pair: Pair<'_, Rule>) -> Result<Link, SchemaError> {
    let mut inner = pair.into_inner();
    let name = inner.next().expect("link name").as_str().to_owned();
    let mut link = Link {
        name,
        from: String::new(),
        to: String::new(),
        link_type: String::new(),
        cardinality: String::new(),
    };
    for property in inner.filter(|pair| pair.as_rule() == Rule::link_property) {
        let mut values = property.into_inner();
        let key = values.next().expect("link property key").as_str();
        let value = values.next().expect("link property value");
        let value = if value.as_rule() == Rule::string {
            parse_string(value.as_str())?
        } else {
            value.as_str().to_owned()
        };
        match key {
            "from" => link.from = value,
            "to" => link.to = value,
            "type" => link.link_type = value,
            "cardinality" => link.cardinality = value,
            _ => {},
        }
    }
    if link.from.is_empty() || link.to.is_empty() {
        return Err(SchemaError::Validation(format!(
            "link `{}` must define `from` and `to`",
            link.name
        )));
    }
    Ok(link)
}

fn parse_value(value: &str) -> Result<String, SchemaError> {
    if value.starts_with('"') {
        parse_string(value)
    } else {
        Ok(value.to_owned())
    }
}

fn parse_string(value: &str) -> Result<String, SchemaError> {
    serde_json::from_str(value)
        .map_err(|error| SchemaError::Parse(format!("invalid string: {error}")))
}

#[cfg(test)]
mod tests {
    use super::{FieldType, parse};

    const SOURCE: &str = r#"version 1
project "shop"

collection users {
  id: string @id
  email: string @unique @index
  status: "active" | "disabled" = "active"
}

collection memories {
  id: string @id
  embedding: vector(1536)?
}

link authored {
  from users
  to memories
  type "authored"
  cardinality many_to_many
}
"#;

    #[test]
    fn parses_and_hashes_schema() {
        let schema = parse(SOURCE).expect("schema should parse");
        assert_eq!(schema.version, 1);
        assert_eq!(schema.collections.len(), 2);
        assert_eq!(
            schema.collections[0].fields[2].field_type,
            FieldType::Enum(vec!["active".into(), "disabled".into()])
        );
        assert!(
            schema
                .hash()
                .expect("hash should work")
                .starts_with("sha256:")
        );
    }

    #[test]
    fn rejects_duplicate_fields() {
        let source = "version 1\ncollection users {\n id: string\n id: string\n}\n";
        assert!(parse(source).is_err());
    }
}
