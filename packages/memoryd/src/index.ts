export type MemoryObject = {
  id: string;
  [key: string]: unknown;
};

export type MemoryEvent = {
  type: string;
  text?: string;
  [key: string]: unknown;
};

export type QueueJobPayload = Record<string, unknown>;

export class MemoryD {
  static async open(path: string): Promise<MemoryD> {
    return new MemoryD(path);
  }

  private constructor(readonly path: string) {}

  async put(collection: string, object: MemoryObject): Promise<void> {
    void collection;
    void object;
    throw new Error("memoryd Node bindings are not implemented yet");
  }

  async get(collection: string, id: string): Promise<MemoryObject | null> {
    void collection;
    void id;
    throw new Error("memoryd Node bindings are not implemented yet");
  }

  events = {
    append: async (stream: string, event: MemoryEvent): Promise<void> => {
      void stream;
      void event;
      throw new Error("memoryd Node bindings are not implemented yet");
    },
  };

  queue(name: string) {
    return {
      push: async (payload: QueueJobPayload): Promise<void> => {
        void name;
        void payload;
        throw new Error("memoryd Node bindings are not implemented yet");
      },
    };
  }
}
