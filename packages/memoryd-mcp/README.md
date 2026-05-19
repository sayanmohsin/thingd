# @sayanmohsin/memoryd-mcp

MCP tools for `memoryd`.

Planned tools:

- `memory.search`
- `memory.get`
- `memory.put`
- `memory.patch`
- `memory.delete`
- `memory.events.append`
- `memory.events.list`
- `memory.queue.push`
- `memory.queue.claim`
- `memory.queue.ack`
- `memory.queue.nack`
- `memory.queue.dead`

The MCP server should wrap the public `@sayanmohsin/memoryd` SDK. It should not reach directly into internal store implementations.
