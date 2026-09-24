type JsonRecord = Record<string, unknown>;

export type AppDefinitionIssue = {
  path: string;
  message: string;
};

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function strings(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string" && item.length > 0);
}

function objectSchema(value: unknown): JsonRecord | undefined {
  return isRecord(value) ? value : undefined;
}

function sourceList(retrieval: JsonRecord): JsonRecord[] | undefined {
  if (Array.isArray(retrieval.sources)) {
    return retrieval.sources.filter(isRecord);
  }
  if (
    typeof retrieval.collection === "string" &&
    strings(retrieval.queryFields) &&
    strings(retrieval.fields) &&
    typeof retrieval.limit === "number"
  ) {
    return [retrieval];
  }
  return undefined;
}

export function validateAppDefinition(value: unknown): AppDefinitionIssue[] {
  const issues: AppDefinitionIssue[] = [];
  const issue = (path: string, message: string) => issues.push({ path, message });
  if (!isRecord(value)) {
    return [{ path: "$", message: "must be a JSON object" }];
  }
  if (value.schemaVersion !== "thingd.app/v1") {
    issue("schemaVersion", "must be thingd.app/v1");
  }
  for (const key of ["name", "slug"]) {
    if (typeof value[key] !== "string" || value[key].trim().length === 0) {
      issue(key, "must be a non-empty string");
    }
  }

  const arrayFields = ["entities", "audiences", "roles", "actions", "views"] as const;
  for (const field of arrayFields) {
    if (!Array.isArray(value[field])) {
      issue(field, "must be an array");
    }
  }
  if (!isRecord(value.policies)) {
    issue("policies", "must be an object");
  }
  if (!isRecord(value.distribution)) {
    issue("distribution", "must be an object");
  }

  const entities = Array.isArray(value.entities) ? value.entities.filter(isRecord) : [];
  const entityByCollection = new Map<string, JsonRecord>();
  for (const [index, entity] of entities.entries()) {
    const path = `entities[${index}]`;
    if (typeof entity.name !== "string" || typeof entity.collection !== "string") {
      issue(path, "requires non-empty name and collection fields");
      continue;
    }
    if (entityByCollection.has(entity.collection)) {
      issue(`${path}.collection`, `duplicates collection '${entity.collection}'`);
    }
    entityByCollection.set(entity.collection, entity);
    if (!Array.isArray(entity.fields)) {
      issue(`${path}.fields`, "must be an array");
    }
  }

  const roles = Array.isArray(value.roles) ? value.roles.filter(isRecord) : [];
  const roleKeys = new Set(
    roles.flatMap((role) => (typeof role.key === "string" ? [role.key] : []))
  );
  const actions = Array.isArray(value.actions) ? value.actions.filter(isRecord) : [];
  const actionKeys = new Set<string>();
  for (const [index, action] of actions.entries()) {
    const path = `actions[${index}]`;
    if (typeof action.key !== "string" || !/^[a-z][a-z0-9_]*$/.test(action.key)) {
      issue(`${path}.key`, "must be a lowercase action key using letters, digits, and underscores");
    } else if (actionKeys.has(action.key)) {
      issue(`${path}.key`, `duplicates action '${action.key}'`);
    } else {
      actionKeys.add(action.key);
    }
    if (typeof action.entity !== "string" || action.entity.length === 0) {
      issue(`${path}.entity`, "must name a declared entity");
    } else if (!entities.some((entity) => entity.name === action.entity)) {
      issue(`${path}.entity`, `references unknown entity '${action.entity}'`);
    }
    if (!objectSchema(action.inputSchema)) {
      issue(`${path}.inputSchema`, "must be an object schema");
    }
    if (Array.isArray(action.allowedRoles)) {
      for (const role of action.allowedRoles) {
        if (typeof role === "string" && !roleKeys.has(role)) {
          issue(`${path}.allowedRoles`, `references unknown role '${role}'`);
        }
      }
    }

    const execution = objectSchema(action.execution);
    if (!execution) {
      continue;
    }
    const executionPath = `${path}.execution`;
    if (!objectSchema(action.outputSchema)) {
      issue(
        `${path}.outputSchema`,
        "is required for an ai_json action and must be an object schema"
      );
    }
    if (execution.kind !== "ai_json") {
      issue(`${executionPath}.kind`, "must be ai_json");
    }
    if (
      !(["agent_reasoning", "complex_agent_planning"] as unknown[]).includes(execution.operation)
    ) {
      issue(`${executionPath}.operation`, "must be agent_reasoning or complex_agent_planning");
    }
    if (
      typeof execution.systemPrompt !== "string" ||
      execution.systemPrompt.length < 1 ||
      execution.systemPrompt.length > 20_000
    ) {
      issue(`${executionPath}.systemPrompt`, "must contain 1 to 20000 characters");
    }
    if (!strings(execution.promptFields)) {
      issue(`${executionPath}.promptFields`, "must be an array of non-empty field names");
    } else {
      const inputProperties = objectSchema(objectSchema(action.inputSchema)?.properties);
      if (inputProperties) {
        for (const field of execution.promptFields) {
          if (!(field in inputProperties)) {
            issue(`${executionPath}.promptFields`, `references undeclared input field '${field}'`);
          }
        }
      }
    }
    const usageLimit = objectSchema(execution.usageLimit);
    if (
      execution.usageLimit !== undefined &&
      (usageLimit?.scope !== "principal" ||
        usageLimit.period !== "lifetime" ||
        !Number.isInteger(usageLimit.limit) ||
        Number(usageLimit.limit) < 1 ||
        Number(usageLimit.limit) > 1_000_000)
    ) {
      issue(`${executionPath}.usageLimit`, "must be a principal lifetime limit from 1 to 1000000");
    }
    if (
      execution.maxOutputTokens !== undefined &&
      (!Number.isInteger(execution.maxOutputTokens) ||
        Number(execution.maxOutputTokens) < 128 ||
        Number(execution.maxOutputTokens) > 8_000)
    ) {
      issue(`${executionPath}.maxOutputTokens`, "must be an integer from 128 to 8000");
    }
    if (
      execution.generatedIdPaths !== undefined &&
      (!Array.isArray(execution.generatedIdPaths) ||
        execution.generatedIdPaths.length > 20 ||
        !execution.generatedIdPaths.every(
          (item) => typeof item === "string" && /^[a-zA-Z0-9_*]+(?:\.[a-zA-Z0-9_*]+)*$/.test(item)
        ))
    ) {
      issue(`${executionPath}.generatedIdPaths`, "must contain at most 20 valid object paths");
    }

    const retrieval = objectSchema(execution.retrieval);
    if (execution.retrieval !== undefined) {
      if (!retrieval) {
        issue(`${executionPath}.retrieval`, "must be an object");
      } else {
        const hasLegacyFields = ["collection", "queryFields", "fields", "limit"].some(
          (key) => key in retrieval
        );
        const sources = sourceList(retrieval);
        if (!sources || (Array.isArray(retrieval.sources) && hasLegacyFields)) {
          issue(
            `${executionPath}.retrieval`,
            "must use either the legacy single source or sources, but not both"
          );
        } else {
          if (sources.length > 8) {
            issue(`${executionPath}.retrieval.sources`, "must contain at most 8 sources");
          }
          for (const [sourceIndex, source] of sources.entries()) {
            const sourcePath = Array.isArray(retrieval.sources)
              ? `${executionPath}.retrieval.sources[${sourceIndex}]`
              : `${executionPath}.retrieval`;
            if (typeof source.collection !== "string" || source.collection.length === 0) {
              issue(`${sourcePath}.collection`, "must be a non-empty collection name");
            }
            if (!strings(source.queryFields) || source.queryFields.length > 12) {
              issue(
                `${sourcePath}.queryFields`,
                "must contain 1 to 12 non-empty input field names"
              );
            } else {
              const inputProperties = objectSchema(objectSchema(action.inputSchema)?.properties);
              if (inputProperties) {
                for (const field of source.queryFields) {
                  if (!(field in inputProperties)) {
                    issue(
                      `${sourcePath}.queryFields`,
                      `references undeclared input field '${field}'`
                    );
                  }
                }
              }
            }
            if (!strings(source.fields) || source.fields.length > 32) {
              issue(`${sourcePath}.fields`, "must contain 1 to 32 non-empty output field names");
            }
            if (
              !Number.isInteger(source.limit) ||
              Number(source.limit) < 1 ||
              Number(source.limit) > 50
            ) {
              issue(`${sourcePath}.limit`, "must be an integer from 1 to 50");
            }
            const entity =
              typeof source.collection === "string"
                ? entityByCollection.get(source.collection)
                : undefined;
            if (!entity) {
              issue(
                `${sourcePath}.collection`,
                `references unpublished collection '${String(source.collection)}'`
              );
            } else if (Array.isArray(entity.fields) && strings(source.fields)) {
              const entityFields = new Set(
                entity.fields
                  .filter(isRecord)
                  .flatMap((field) => (typeof field.name === "string" ? [field.name] : []))
              );
              for (const field of source.fields) {
                if (!entityFields.has(field)) {
                  issue(
                    `${sourcePath}.fields`,
                    `references undeclared collection field '${field}'`
                  );
                }
              }
            }
          }
        }
      }
    }

    const write = objectSchema(execution.write);
    if (execution.write !== undefined) {
      if (!write || typeof write.collection !== "string") {
        issue(`${executionPath}.write`, "requires a collection name");
      } else {
        const entity = entityByCollection.get(write.collection);
        if (!entity) {
          issue(
            `${executionPath}.write.collection`,
            `references unpublished collection '${write.collection}'`
          );
        }
        if (write.ownerField !== undefined && typeof write.ownerField !== "string") {
          issue(`${executionPath}.write.ownerField`, "must be a field name");
        } else if (typeof write.ownerField === "string" && entity) {
          const entityFields = Array.isArray(entity.fields) ? entity.fields.filter(isRecord) : [];
          if (!entityFields.some((field) => field.name === write.ownerField)) {
            issue(
              `${executionPath}.write.ownerField`,
              `references undeclared collection field '${write.ownerField}'`
            );
          }
        }
        if (
          entity &&
          typeof entity.ownershipField === "string" &&
          write.ownerField !== entity.ownershipField
        ) {
          issue(
            `${executionPath}.write.ownerField`,
            `must match the entity ownership field '${entity.ownershipField}'`
          );
        }
        if (
          entity &&
          typeof entity.ownershipField !== "string" &&
          typeof write.ownerField === "string"
        ) {
          issue(
            `${executionPath}.write.ownerField`,
            "cannot be set for an entity without ownership"
          );
        }
        for (const field of [write.createdAtField, write.updatedAtField]) {
          if (
            field !== undefined &&
            (typeof field !== "string" ||
              !Array.isArray(entity?.fields) ||
              !entity.fields.filter(isRecord).some((candidate) => candidate.name === field))
          ) {
            issue(
              `${executionPath}.write`,
              `references undeclared write timestamp field '${String(field)}'`
            );
          }
        }
      }
    }
  }
  return issues;
}

export function canonicalJson(value: unknown): string {
  const normalize = (input: unknown): unknown => {
    if (Array.isArray(input)) {
      return input.map(normalize);
    }
    if (!isRecord(input)) {
      return input;
    }
    return Object.fromEntries(
      Object.keys(input)
        .sort()
        .map((key) => [key, normalize(input[key])])
    );
  };
  return JSON.stringify(normalize(value));
}
