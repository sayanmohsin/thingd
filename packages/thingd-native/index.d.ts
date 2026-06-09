export class NativeThingStore {
  static open(path: string): NativeThingStore;

  putObjectJson(collection: string, id: string, body: string): string;
  getObjectJson(collection: string, id: string): string | null;
  listObjectsJson(collectionsJson?: string): string;
  deleteObject(collection: string, id: string): boolean;
  appendEventJson(stream: string, body: string): string;
  listEventsJson(stream?: string): string;
  pushJobJson(
    queue: string,
    id: string,
    body: string,
    maxAttempts: number,
    delayMs: number
  ): string;
  claimJobJson(queue: string, leaseMs: number): string | null;
  ackJobJson(queue: string, id: string): string;
  nackJobJson(queue: string, id: string, delayMs: number): string;
  listJobsJson(queue: string): string;
  listDeadJobsJson(queue: string): string;
  countObjectsJson(): Promise<number>;
  countEventsJson(): Promise<number>;
  countActiveJobsJson(): Promise<number>;
  countDeadJobsJson(): Promise<number>;
  listCollectionsJson(): Promise<string>;
  listStreamsJson(): Promise<string>;
}
