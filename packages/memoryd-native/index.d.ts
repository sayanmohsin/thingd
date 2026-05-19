export class NativeMemoryStore {
  static open(path: string): NativeMemoryStore;

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
    delayMs: number,
  ): string;
  claimJobJson(queue: string, leaseMs: number): string | null;
  ackJobJson(queue: string, id: string): string;
  nackJobJson(queue: string, id: string, delayMs: number): string;
  listJobsJson(queue: string): string;
  listDeadJobsJson(queue: string): string;
}
