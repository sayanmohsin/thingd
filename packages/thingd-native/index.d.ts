export class NativeThingStore {
  static open(path: string, encryptionKey?: string): NativeThingStore;
  static reencrypt(
    sourcePath: string,
    destinationPath: string,
    sourceKey?: string,
    destinationKey?: string,
    allowPlaintextOutput?: boolean
  ): void;

  getSchemaDocumentJson(): string | null;
  putSchemaDocumentJson(schemaJson: string, hash: string, updatedAt: string): void;
  listMigrationsJson(): string;
  recordMigrationJson(id: string, hash: string, appliedAt: string): void;

  putObjectJson(collection: string, id: string, body: string): string;
  getObjectJson(collection: string, id: string): string | null;
  getObjectsBatchJson(collection: string, ids: string[]): string;
  listObjectsJson(
    collectionsJson?: string,
    filterJson?: string,
    limit?: number,
    offset?: number,
    sortField?: string,
    sortDirection?: string
  ): Promise<string>;
  deleteObject(collection: string, id: string): boolean;
  appendEventJson(stream: string, body: string): string;
  listEventsJson(stream?: string, fromSequence?: number, limit?: number, since?: string): string;
  pushJobJson(
    queue: string,
    id: string,
    body: string,
    maxAttempts: number,
    delayMs: number,
    priority?: number
  ): string;
  claimJobJson(queue: string, leaseMs: number): string | null;
  ackJobJson(queue: string, id: string): string;
  nackJobJson(queue: string, id: string, delayMs: number, error?: string): string;
  listJobsJson(queue: string): string;
  listDeadJobsJson(queue: string): string;
  listQueuesJson(): string;
  putObjectsBatchJson(objectsJson: string): string;
  appendEventsBatchJson(eventsJson: string): string;
  pushJobsBatchJson(jobsJson: string): string;
  searchJson(query: string, collectionsJson?: string, limit?: number, filterJson?: string): string;
  deleteObjectsBatchJson(keysJson: string): number;
  createLinkJson(
    fromRef: string,
    linkType: string,
    toRef: string,
    weight?: number,
    metadataJson?: string
  ): string;
  deleteLink(id: string): boolean;
  getLinkJson(id: string): string | null;
  getNeighborsJson(reference: string, direction: string, linkType?: string, limit?: number): string;
  countObjectsJson(): Promise<number>;
  countObjectsInCollectionJson(collection: string): Promise<number>;
  countEventsJson(): Promise<number>;
  countActiveJobsJson(): Promise<number>;
  countDeadJobsJson(): Promise<number>;
  countLinksJson(): Promise<number>;
  listCollectionsJson(): Promise<string>;
  listStreamsJson(): Promise<string>;
  createIndexJson(collection: string, field: string): void;
  createUniqueIndexJson(collection: string, field: string): void;
  deleteIndexJson(collection: string, field: string): boolean;
  listIndexesJson(): string;
  aggregateJson(
    collection: string,
    function_: string,
    field?: string,
    groupBy?: string,
    filterJson?: string
  ): string;
  timeseriesJson(
    collection: string,
    function_: string,
    field?: string,
    bucket?: string,
    from?: string,
    to?: string,
    filterJson?: string
  ): string;
}

export function parseSchema(source: string): string;
