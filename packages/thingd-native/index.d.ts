export class NativeThingStore {
  static open(path: string): NativeThingStore;

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
    delayMs: number
  ): string;
  claimJobJson(queue: string, leaseMs: number): string | null;
  ackJobJson(queue: string, id: string): string;
  nackJobJson(queue: string, id: string, delayMs: number, error?: string): string;
  listJobsJson(queue: string): string;
  listDeadJobsJson(queue: string): string;
  listQueuesJson(): string;
  walCheckpoint(): string;
  backupTo(path: string): void;
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
  countEventsJson(): Promise<number>;
  countActiveJobsJson(): Promise<number>;
  countDeadJobsJson(): Promise<number>;
  countLinksJson(): Promise<number>;
  listCollectionsJson(): Promise<string>;
  listStreamsJson(): Promise<string>;
  createIndexJson(collection: string, field: string): void;
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
