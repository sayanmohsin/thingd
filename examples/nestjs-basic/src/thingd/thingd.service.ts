import { randomUUID } from "node:crypto";
import { Injectable } from "@nestjs/common";

type MemoryObject = {
  id: string;
  [key: string]: unknown;
};

type MemoryEvent = {
  type: string;
  text?: string;
  [key: string]: unknown;
};

type QueueJob = {
  id: string;
  queue: string;
  payload: Record<string, unknown>;
  status: "ready";
  attempts: number;
  createdAt: string;
};

@Injectable()
export class ThingdService {
  private readonly collections = new Map<string, Map<string, MemoryObject>>();
  private readonly events: Array<MemoryEvent & { stream: string; createdAt: string }> = [];
  private readonly queues = new Map<string, QueueJob[]>();

  put(collection: string, object: MemoryObject): MemoryObject {
    const records = this.collections.get(collection) ?? new Map<string, MemoryObject>();
    const record = {
      ...object,
      id: object.id,
      updatedAt: new Date().toISOString(),
    };

    records.set(record.id, record);
    this.collections.set(collection, records);

    return record;
  }

  get(collection: string, id: string): MemoryObject | null {
    return this.collections.get(collection)?.get(id) ?? null;
  }

  appendEvent(stream: string, event: MemoryEvent) {
    const record = {
      ...event,
      stream,
      createdAt: new Date().toISOString(),
    };

    this.events.push(record);
    return record;
  }

  pushJob(queue: string, payload: Record<string, unknown>): QueueJob {
    const jobs = this.queues.get(queue) ?? [];
    const job: QueueJob = {
      id: randomUUID(),
      queue,
      payload,
      status: "ready",
      attempts: 0,
      createdAt: new Date().toISOString(),
    };

    jobs.push(job);
    this.queues.set(queue, jobs);

    return job;
  }

  listJobs(queue: string): QueueJob[] {
    return this.queues.get(queue) ?? [];
  }
}
