# Schema files

`schema.thingd` is an optional, version-controlled schema document for projects
that want explicit collections, fields, indexes, search annotations, and links.
Existing applications can continue using thingd without a schema file.

```thingd
version 1

project "shop"

collection users {
  id: string @id
  email: string @unique @index
  status: "active" | "disabled" = "active"
  createdAt: datetime @default(now)
}

collection memories {
  id: string @id
  content: string @searchable
  embedding: vector(1536)?
}

link authored {
  from users
  to memories
  type "authored"
  cardinality many_to_many
}
```

The parser is implemented in Rust in `thingd-schema` and exposed through the
native Node binding. Parsing produces a canonical JSON representation and a
stable SHA-256 schema hash. This gives tooling a deterministic basis for later
migration planning without changing the runtime's schemaless object behavior.

## Check a schema

```bash
thingd schema check schema.thingd
```

The command reports the absolute file path, canonical schema document, and
schema hash. Omitting the path checks `schema.thingd` in the current directory.

The current preview validates syntax, duplicate collections and fields, vector
dimensions, and link endpoints. Migration creation, planning, and application
will build on this same canonical representation in a later phase.

## Migration planning

Create a numbered migration from a validated schema:

```bash
thingd migrate create initial --schema schema.thingd
thingd migrate status
thingd migrate plan schema.thingd
```

Planning is advisory and non-destructive. It reports collection and field
additions/removals against the current inferred runtime schema and marks
removals as destructive. Applying a migration persists its canonical schema
hash and migration record, and creates `@index`/`@unique` indexes without
deleting existing objects. Rollback and destructive schema changes are not
enabled yet.

## Supported declarations

Initial field types are `string`, `number`, `boolean`, `datetime`, `json`,
`vector(dimensions)`, arrays such as `string[]`, and quoted enum values. The
first annotations are `@id`, `@unique`, `@index`, `@searchable`, `@default`,
`@readonly`, and `@nullable`.

Schema files are optional by design: use them when a repository benefits from
explicit, reviewable data contracts; keep using inferred runtime schema when a
prototype needs maximum flexibility.
