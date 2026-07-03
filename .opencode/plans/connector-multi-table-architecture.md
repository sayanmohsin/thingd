# Connector Multi-Table Architecture

## Core concept

**One connector = one database.** Each connector can sync multiple tables into
separate thingd collections, with schema annotations for NLQ.

```
Connector "prod-db" (Postgres: prod.example.com:5432)
  │
  ├── TableSync "orders"    → "orders"    collection  🟢 active
  ├── TableSync "customers" → "customers" collection  🟢 active
  └── TableSync "products"  → "products"  collection  🔴 paused
```

---

## Data model

### Connector (already exists — add `tableSyncs` relation)

```typescript
ConnectorDto {
  id: string;
  projectId: string;
  name: string;
  databaseType: "postgres" | "mysql";
  instanceId: string;
  connectionString: string;      // vault-stored
  status: "active" | "paused" | "error";
  tableCount: number;             // NEW — count of table syncs
  lastSyncAt: string | undefined; // NEW — latest across all tables
  createdAt: string;
  updatedAt: string;
}
```

### TableSync (new — one per synced table)

```typescript
TableSyncDto {
  id: string;
  connectorId: string;
  tableName: string;              // e.g. "orders"
  collection: string;             // thingd collection name
  description: string;            // "Customer orders with line items"
  query: string;                  // "SELECT * FROM orders"
  schedule: string | undefined;   // cron
  status: "active" | "paused" | "error";
  lastSyncAt: string | undefined;
  lastSyncRows: number | undefined;
  lastSyncError: string | undefined;
  columnAnnotations: ColumnAnnotation[];
  createdAt: string;
  updatedAt: string;
}
```

### ColumnAnnotation (new — for NLQ schema)

```typescript
ColumnAnnotation {
  name: string;
  description: string;            // "Order total in USD"
  primaryKey: boolean;
  foreignKey: { table: string; column: string } | undefined;
  type: "text" | "integer" | "float" | "boolean" | "timestamp" | "currency"
        | "enum" | "json" | "unknown";
  enumValues: string[] | undefined;  // for "pending","shipped","canceled"
  included: boolean;              // false = exclude from NLQ
}
```

---

## API endpoints

All under `/projects/:projectId/connectors/:connectorId/tables`

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/tables` | List all TableSyncs for this connector |
| POST | `/tables` | Create one or more TableSyncs from a table list |
| GET | `/tables/:tableId` | Get a single TableSync with annotations |
| PATCH | `/tables/:tableId` | Update table config, description, annotations |
| DELETE | `/tables/:tableId` | Remove a table sync (stop syncing) |
| POST | `/tables/:tableId/sync` | Trigger sync for a single table |
| POST | `/tables/:tableId/schema` | Re-discover schema + merge annotations |
| GET | `/tables/:tableId/history` | Sync history for this table |

### Bulk create endpoint

`POST /tables` accepts:

```json
{
  "tables": [
    {
      "tableName": "orders",
      "collection": "orders",
      "description": "Customer orders with line items",
      "schedule": "0 */6 * * *"
    },
    {
      "tableName": "customers",
      "collection": "customers",
      "description": "Customer profiles and contact info"
    }
  ]
}
```

Returns all created TableSyncs with auto-discovered column annotations.

### Preview tables (already exists)

`POST /projects/connectors/preview-tables`

Returns available tables from the database. Now extended to also return
auto-discovered columns per table for the annotation step.

---

## UI flow (3-step wizard)

### Step 1 — Connect (existing, simplified)

| Field | Type |
|-------|------|
| Connector name | text (e.g. "Production DB") |
| Database type | dropdown (Postgres / MySQL) |
| Connection string | secret text |
| Agent type | dropdown |
| Target instance | dropdown |

### Step 2 — Pick tables

After connecting, shows a table list with checkboxes:

```
┌─────────────────────────────────────────────┐
│ Tables in Production DB                      │
│                                             │
│ ☑ orders        (15 cols)  →  orders       │
│ ☑ customers     (8 cols)   →  customers    │
│ ☑ products      (12 cols)  →  products     │
│ ☐ inventory     (6 cols)                   │
│ ☐ reviews       (4 cols)                   │
│                                             │
│ Each collection name is auto-filled from    │
│ the table name. Editable.                   │
│                                             │
│          [Cancel]    [Next: Describe]       │
└─────────────────────────────────────────────┘
```

Multi-select by default. Collection name auto-filled from table name,
user can edit inline.

### Step 3 — Describe data

For each selected table, show an annotation card:

```
┌──────────────────────────────────────────────────────────┐
│ orders ───→ orders collection                             │
│ "Customer orders with line items and totals"              │
│                                                          │
│ Columns:                                                  │
│   ☑ id          "Unique order ID"              PRIMARY   │
│   ☑ customer_id "References customers.id"      FK → cust │
│   ☑ total       "Total amount in USD"          💰 float  │
│   ☑ status      "pending│shipped│canceled"     enum      │
│   ☑ created_at  "Order placement date"         📅        │
│   ☐ internal_note "" (excluded)                          │
│                                                          │
│          [Back]                        [Create 3 Tables] │
└──────────────────────────────────────────────────────────┘
```

Auto-discovery:
- Column names → human-readable descriptions via LLM (or manual edit)
- Data types mapped from SQL type
- Primary keys detected from information_schema
- Foreign keys detected (falls back to `_id`/`_key` naming patterns)
- Enum values sampled from first 100 rows
- Checkboxes default to all included (user unchecks sensitive/internal columns)

### Connector detail page

After creation, the connector detail shows:

```
Connector: Production DB
Status: 🟢 Active    Last sync: 2m ago

Tables (3 synced):
┌──────────┬────────────┬──────────┬───────────┬──────────────┐
│ Table    │ Collection │ Status   │ Last sync │ Rows         │
├──────────┼────────────┼──────────┼───────────┼──────────────┤
│ orders   │ orders     │ 🟢       │ 2m ago    │ 15,234       │
│ customers│ customers  │ 🟢       │ 5m ago    │ 4,567        │
│ products │ products   │ 🔴 error │ 1h ago    │ FAILED       │
└──────────┴────────────┴──────────┴───────────┴──────────────┘

Actions per table: [Sync] [Edit] [History] [Delete]
Connector actions: [Sync All] [Edit Connector] [Delete Connector]
```

---

## NLQ integration path

The `columnAnnotations` on each TableSync feed into a future `__thingd:schemas`
collection, consumed by the NLQ query engine:

```
thing_query "What were our top 10 orders last month?"
  │
  ├─ Reads schemas from __thingd:schemas
  │   → "orders" has total, created_at, status columns
  │   → "customers" has name, email columns
  │
  ├─ Generates: SELECT * FROM orders
  │   WHERE status != 'canceled'
  │   AND created_at >= '2026-06-01'
  │   ORDER BY total DESC LIMIT 10
  │
  └─ Returns: [top 10 orders with customer names joined]
```

---

## Implementation order

| Step | What | Files | Est. |
|------|------|-------|------|
| 1 | `TableSync` types + `ColumnAnnotation` | `packages/shared/src/index.ts` | 30 min |
| 2 | Backend: CRUD for table syncs | `apps/src/connector/` | 1 hr |
| 3 | Backend: Bulk create endpoint | `controller.ts`, `service.ts` | 30 min |
| 4 | Backend: Column auto-discovery from `information_schema` | `service.ts` | 1 hr |
| 5 | Frontend: Step 2 multi-select table picker | `ProjectConnectors.tsx` | 1 hr |
| 6 | Frontend: Step 3 annotation wizard | `ProjectConnectorWizard.tsx` (new) | 2 hr |
| 7 | Frontend: Connector detail with table list | `ProjectConnectorDetail.tsx` (new) | 1 hr |
| 8 | Frontend: Sync All button | `ProjectConnectors.tsx` | 30 min |
| **Total** | | | **~7.5 hr** |
