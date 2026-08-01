/**
 * Compile-time type tests for the SDK public type surface.
 * This file is only checked by tsc — it produces no runtime output.
 *
 * If any import fails to resolve, the build will break, catching missing
 * barrel exports automatically.
 */

// ── Main barrel (@thingd/sdk) ──
import type {
  AggregateFunction,
  AggregateGroupResult,
  AggregateOptions,
  AggregateResult,
  BackupCapableThingStore,
  CollectionSchema,
  ConnectorAuth,
  ConnectorSchema,
  ConnectorSyncOptions,
  ConnectorSyncResult,
  FieldSchema,
  FilterOperator,
  Link,
  LinkDirection,
  LinkQueryOptions,
  ListEventsOptions,
  ListObjectsOptions,
  LocalThingDConnection,
  MemoryEvent,
  MemoryObject,
  MemoryQueue,
  MemorySearchOptions,
  MemorySearchResult,
  NlqIntent,
  NlqOptions,
  NlqResult,
  PutOptions,
  QueueClaimOptions,
  QueueJob,
  QueueJobOptions,
  QueueJobPayload,
  QueueJobResult,
  QueueJobStatus,
  QueueNackOptions,
  ReconnectableThingStore,
  Schedule,
  ScheduleContext,
  ScheduleEvent,
  ScheduleHandler,
  ScheduleIntervalOptions,
  ScheduleOnceOptions,
  ScheduleOptions,
  SchedulerEventType,
  SchedulerFacade,
  SchedulerListener,
  SchedulerStats,
  SchemaOptions,
  SortBy,
  SortDirection,
  StoredMemoryEvent,
  StoredMemoryObject,
  ThingDConnection,
  ThingDeleteResult,
  ThingStore,
  TimeBucket,
  TimeSeriesBucket,
  TimeSeriesOptions,
  TimeSeriesResult,
  VectorSearchHit,
  VectorSearchOptions,
  WalCheckpointResult,
} from "../index.js";

// ── Type-only barrel (@thingd/sdk/types) ──
import type {
  AggregateFunction as TAggregateFunction,
  AggregateGroupResult as TAggregateGroupResult,
  AggregateOptions as TAggregateOptions,
  AggregateResult as TAggregateResult,
  BackupCapableThingStore as TBackupCapableThingStore,
  CollectionSchema as TCollectionSchema,
  ConnectorAuth as TConnectorAuth,
  ConnectorSchema as TConnectorSchema,
  ConnectorSyncOptions as TConnectorSyncOptions,
  ConnectorSyncResult as TConnectorSyncResult,
  FieldSchema as TFieldSchema,
  FilterOperator as TFilterOperator,
  Link as TLink,
  LinkDirection as TLinkDirection,
  LinkQueryOptions as TLinkQueryOptions,
  ListEventsOptions as TListEventsOptions,
  ListObjectsOptions as TListObjectsOptions,
  MemoryEvent as TMemoryEvent,
  MemoryObject as TMemoryObject,
  MemoryQueue as TMemoryQueue,
  MemorySearchOptions as TMemorySearchOptions,
  MemorySearchResult as TMemorySearchResult,
  NlqIntent as TNlqIntent,
  NlqOptions as TNlqOptions,
  NlqResult as TNlqResult,
  PutOptions as TPutOptions,
  QueueClaimOptions as TQueueClaimOptions,
  QueueJob as TQueueJob,
  QueueJobOptions as TQueueJobOptions,
  QueueJobPayload as TQueueJobPayload,
  QueueJobResult as TQueueJobResult,
  QueueJobStatus as TQueueJobStatus,
  QueueNackOptions as TQueueNackOptions,
  ReconnectableThingStore as TReconnectableThingStore,
  SchemaOptions as TSchemaOptions,
  SortBy as TSortBy,
  SortDirection as TSortDirection,
  StoredMemoryEvent as TStoredMemoryEvent,
  StoredMemoryObject as TStoredMemoryObject,
  ThingDConnection as TThingDConnection,
  LocalThingDConnection as TLocalThingDConnection,
  ThingDeleteResult as TThingDeleteResult,
  ThingStore as TThingStore,
  TimeBucket as TTimeBucket,
  TimeSeriesBucket as TTimeSeriesBucket,
  TimeSeriesOptions as TTimeSeriesOptions,
  TimeSeriesResult as TTimeSeriesResult,
  VectorSearchHit as TVectorSearchHit,
  VectorSearchOptions as TVectorSearchOptions,
  WalCheckpointResult as TWalCheckpointResult,
} from "../types/index.js";

// ── Client barrel (@thingd/sdk/client) ──
import type {
  AggregateFunction as CAggregateFunction,
  AggregateGroupResult as CAggregateGroupResult,
  AggregateOptions as CAggregateOptions,
  AggregateResult as CAggregateResult,
  BackupCapableThingStore as CBackupCapableThingStore,
  CollectionSchema as CCollectionSchema,
  ConnectorAuth as CConnectorAuth,
  ConnectorSchema as CConnectorSchema,
  ConnectorSyncOptions as CConnectorSyncOptions,
  ConnectorSyncResult as CConnectorSyncResult,
  FieldSchema as CFieldSchema,
  FilterOperator as CFilterOperator,
  Link as CLink,
  LinkDirection as CLinkDirection,
  LinkQueryOptions as CLinkQueryOptions,
  ListEventsOptions as CListEventsOptions,
  ListObjectsOptions as CListObjectsOptions,
  MemoryEvent as CMemoryEvent,
  MemoryObject as CMemoryObject,
  MemoryQueue as CMemoryQueue,
  MemorySearchOptions as CMemorySearchOptions,
  MemorySearchResult as CMemorySearchResult,
  NlqIntent as CNlqIntent,
  NlqOptions as CNlqOptions,
  NlqResult as CNlqResult,
  PutOptions as CPutOptions,
  QueueClaimOptions as CQueueClaimOptions,
  QueueJob as CQueueJob,
  QueueJobOptions as CQueueJobOptions,
  QueueJobPayload as CQueueJobPayload,
  QueueJobResult as CQueueJobResult,
  QueueJobStatus as CQueueJobStatus,
  QueueNackOptions as CQueueNackOptions,
  ReconnectableThingStore as CReconnectableThingStore,
  SchemaOptions as CSchemaOptions,
  SortBy as CSortBy,
  SortDirection as CSortDirection,
  StoredMemoryEvent as CStoredMemoryEvent,
  StoredMemoryObject as CStoredMemoryObject,
  ThingDConnection as CThingDConnection,
  LocalThingDConnection as CLocalThingDConnection,
  ThingDeleteResult as CThingDeleteResult,
  ThingStore as CThingStore,
  TimeBucket as CTimeBucket,
  TimeSeriesBucket as CTimeSeriesBucket,
  TimeSeriesOptions as CTimeSeriesOptions,
  TimeSeriesResult as CTimeSeriesResult,
  VectorSearchHit as CVectorSearchHit,
  VectorSearchOptions as CVectorSearchOptions,
  WalCheckpointResult as CWalCheckpointResult,
} from "../client/index.js";

// ── Use every imported type to suppress unused-import errors ──
// Exporting the type aliases ensures tsc considers them "used".
export type _MainExports = {
  _af: AggregateFunction;
  _agr: AggregateGroupResult;
  _ao: AggregateOptions;
  _ar: AggregateResult;
  _bcts: BackupCapableThingStore;
  _cs: CollectionSchema;
  _ca: ConnectorAuth;
  _cs2: ConnectorSchema;
  _cso: ConnectorSyncOptions;
  _csr: ConnectorSyncResult;
  _fs: FieldSchema;
  _fo: FilterOperator;
  _l: Link;
  _ld: LinkDirection;
  _lqo: LinkQueryOptions;
  _leo: ListEventsOptions;
  _loo: ListObjectsOptions;
  _me: MemoryEvent;
  _mo: MemoryObject;
  _mq: MemoryQueue;
  _mso: MemorySearchOptions;
  _msr: MemorySearchResult;
  _ni: NlqIntent;
  _no: NlqOptions;
  _nr: NlqResult;
  _po: PutOptions;
  _qco: QueueClaimOptions;
  _qj: QueueJob;
  _qjo: QueueJobOptions;
  _qjp: QueueJobPayload;
  _qjr: QueueJobResult;
  _qjs: QueueJobStatus;
  _qno: QueueNackOptions;
  _rcts: ReconnectableThingStore;
  _s: Schedule;
  _sc: ScheduleContext;
  _se: ScheduleEvent;
  _sh: ScheduleHandler;
  _sio: ScheduleIntervalOptions;
  _so2: ScheduleOnceOptions;
  _so3: ScheduleOptions;
  _set: SchedulerEventType;
  _sf: SchedulerFacade;
  _sl: SchedulerListener;
  _ss: SchedulerStats;
  _so4: SchemaOptions;
  _sb: SortBy;
  _sd: SortDirection;
  _sme: StoredMemoryEvent;
  _smo: StoredMemoryObject;
  _tdc: ThingDConnection;
  _ltdc: LocalThingDConnection;
  _tdr: ThingDeleteResult;
  _ts: ThingStore;
  _tb: TimeBucket;
  _tsb: TimeSeriesBucket;
  _tso: TimeSeriesOptions;
  _tsr: TimeSeriesResult;
  _vsh: VectorSearchHit;
  _vso: VectorSearchOptions;
  _wcr: WalCheckpointResult;
};

export type _TypeExports = {
  _af: TAggregateFunction;
  _agr: TAggregateGroupResult;
  _ao: TAggregateOptions;
  _ar: TAggregateResult;
  _bcts: TBackupCapableThingStore;
  _cs: TCollectionSchema;
  _ca: TConnectorAuth;
  _cs2: TConnectorSchema;
  _cso: TConnectorSyncOptions;
  _csr: TConnectorSyncResult;
  _fs: TFieldSchema;
  _fo: TFilterOperator;
  _l: TLink;
  _ld: TLinkDirection;
  _lqo: TLinkQueryOptions;
  _leo: TListEventsOptions;
  _loo: TListObjectsOptions;
  _me: TMemoryEvent;
  _mo: TMemoryObject;
  _mq: TMemoryQueue;
  _mso: TMemorySearchOptions;
  _msr: TMemorySearchResult;
  _ni: TNlqIntent;
  _no: TNlqOptions;
  _nr: TNlqResult;
  _po: TPutOptions;
  _qco: TQueueClaimOptions;
  _qj: TQueueJob;
  _qjo: TQueueJobOptions;
  _qjp: TQueueJobPayload;
  _qjr: TQueueJobResult;
  _qjs: TQueueJobStatus;
  _qno: TQueueNackOptions;
  _rcts: TReconnectableThingStore;
  _so4: TSchemaOptions;
  _sb: TSortBy;
  _sd: TSortDirection;
  _sme: TStoredMemoryEvent;
  _smo: TStoredMemoryObject;
  _tdc: TThingDConnection;
  _ltdc: TLocalThingDConnection;
  _tdr: TThingDeleteResult;
  _ts: TThingStore;
  _tb: TTimeBucket;
  _tsb: TTimeSeriesBucket;
  _tso: TTimeSeriesOptions;
  _tsr: TTimeSeriesResult;
  _vsh: TVectorSearchHit;
  _vso: TVectorSearchOptions;
  _wcr: TWalCheckpointResult;
};

export type _ClientExports = {
  _af: CAggregateFunction;
  _agr: CAggregateGroupResult;
  _ao: CAggregateOptions;
  _ar: CAggregateResult;
  _bcts: CBackupCapableThingStore;
  _cs: CCollectionSchema;
  _ca: CConnectorAuth;
  _cs2: CConnectorSchema;
  _cso: CConnectorSyncOptions;
  _csr: CConnectorSyncResult;
  _fs: CFieldSchema;
  _fo: CFilterOperator;
  _l: CLink;
  _ld: CLinkDirection;
  _lqo: CLinkQueryOptions;
  _leo: CListEventsOptions;
  _loo: CListObjectsOptions;
  _me: CMemoryEvent;
  _mo: CMemoryObject;
  _mq: CMemoryQueue;
  _mso: CMemorySearchOptions;
  _msr: CMemorySearchResult;
  _ni: CNlqIntent;
  _no: CNlqOptions;
  _nr: CNlqResult;
  _po: CPutOptions;
  _qco: CQueueClaimOptions;
  _qj: CQueueJob;
  _qjo: CQueueJobOptions;
  _qjp: CQueueJobPayload;
  _qjr: CQueueJobResult;
  _qjs: CQueueJobStatus;
  _qno: CQueueNackOptions;
  _rcts: CReconnectableThingStore;
  _so4: CSchemaOptions;
  _sb: CSortBy;
  _sd: CSortDirection;
  _sme: CStoredMemoryEvent;
  _smo: CStoredMemoryObject;
  _tdc: CThingDConnection;
  _ltdc: CLocalThingDConnection;
  _tdr: CThingDeleteResult;
  _ts: CThingStore;
  _tb: CTimeBucket;
  _tsb: CTimeSeriesBucket;
  _tso: CTimeSeriesOptions;
  _tsr: CTimeSeriesResult;
  _vsh: CVectorSearchHit;
  _vso: CVectorSearchOptions;
  _wcr: CWalCheckpointResult;
};

// ── Capability interface checks ──
declare const reconnectableStore: ReconnectableThingStore;
void reconnectableStore.reconnect(); // must return Promise<void>

declare const backupStore: BackupCapableThingStore;
backupStore.backupTo("/tmp/backup.db"); // must return void
const walResult: WalCheckpointResult = backupStore.walCheckpoint();
void walResult.framesBefore; // number
void walResult.framesAfter; // number

// ── LocalThingDConnection has backupTo and walCheckpoint ──
declare const conn: LocalThingDConnection;
conn.backupTo("/tmp/backup.db");
const connWal: WalCheckpointResult = conn.walCheckpoint();
void connWal.framesBefore;
void connWal.framesAfter;

// ── FilterOperator usage ──
const filterOp: FilterOperator = { $gt: 10, $in: ["a", "b"], $like: "%test%" };
void filterOp;

// ── Aggregate types ──
const aggFn: AggregateFunction = "count";
const aggOpts: AggregateOptions = { function: "sum", field: "price", groupBy: "category" };
const aggGroup: AggregateGroupResult = { key: "electronics", value: 42 };
const aggResult: AggregateResult = { total: 100, groups: [aggGroup] };
void aggFn;
void aggOpts;
void aggResult;

// ── TimeSeries types ──
const timeBucket: TimeBucket = "day";
const tsOpts: TimeSeriesOptions = { function: "avg", field: "price", bucket: "week" };
const tsBucket: TimeSeriesBucket = { label: "2026-W01", value: 99.9 };
const tsResult: TimeSeriesResult = { buckets: [tsBucket] };
void timeBucket;
void tsOpts;
void tsResult;

// ── Schema types ──
const fieldSchema: FieldSchema = { name: "title", type: "string", nullable: false, sampleValues: [] };
const colSchema: CollectionSchema = { name: "posts", objectCount: 10, fields: [fieldSchema] };
const schemaOpts: SchemaOptions = { sampleSize: 100 };
void fieldSchema;
void colSchema;
void schemaOpts;

// ── NLQ types ──
const nlqIntent: NlqIntent = { action: "aggregate", collection: "orders", function: "sum" };
const nlqResult: NlqResult = { answer: "Total is $500", data: { total: 500 }, intent: nlqIntent };
const nlqOpts: NlqOptions = { collection: "orders", model: "gpt-4" };
void nlqIntent;
void nlqResult;
void nlqOpts;

// ── Vector types ──
const vecOpts: VectorSearchOptions = { topK: 5, filter: { category: "books" } };
const vecHit: VectorSearchHit = {
  id: "obj-1",
  score: 0.95,
  value: { id: "obj-1", collection: "books", createdAt: "", updatedAt: "", version: 1 },
};
void vecOpts;
void vecHit;

// ── Connector types ──
const connAuth: ConnectorAuth = {
  host: "localhost",
  port: 5432,
  database: "mydb",
  username: "user",
  password: "pass",
};
const connSchema: ConnectorSchema = {
  name: "users",
  columns: [{ name: "id", dataType: "integer", nullable: false, sampleValues: [] }],
  estimatedRows: 1000,
};
const connSyncOpts: ConnectorSyncOptions = {
  collection: "users",
  query: "SELECT * FROM users",
  auth: connAuth,
};
const connSyncResult: ConnectorSyncResult = { imported: 100, collection: "users" };
void connAuth;
void connSchema;
void connSyncOpts;
void connSyncResult;
