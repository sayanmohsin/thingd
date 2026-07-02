import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';
import { readFile } from 'node:fs/promises';
import path from 'node:path';

const mcpUrl = 'https://api.thingd.cloud/mcp/proj_YMEkZYgKEjMsHuZt/dkp';
const authToken = 'md_test_9372dd932b8a1b2d6282_8LdW-7eSNHpL40HJ2n0vPZau_gCSFCeQ';
const baseDir = '/Users/sayanmohsin/Space/Programming/personal/thingd/examples/sales-db';

const datasets = [
  { collection: 'customers', file: 'customers.json' },
  { collection: 'products', file: 'products.json' },
  { collection: 'orders', file: 'orders.json' },
  { collection: 'order_items', file: 'order_items.json' },
];

const client = new Client({ name: 'thingd-sales-seed', version: '0.1.0' });
const transport = new StreamableHTTPClientTransport(new URL(mcpUrl), {
  requestInit: {
    headers: {
      Authorization: `Bearer ${authToken}`,
    },
  },
});

try {
  await client.connect(transport);
  for (const dataset of datasets) {
    const raw = await readFile(path.join(baseDir, dataset.file), 'utf8');
    const objects = JSON.parse(raw);
    const result = await client.callTool({
      name: 'thing_objects_put_batch',
      arguments: {
        collection: dataset.collection,
        objects,
      },
    });
    console.log(`${dataset.collection}: ${JSON.stringify(result)}`);
  }

  const listed = await client.callTool({
    name: 'thing_list_collections',
    arguments: {},
  });
  console.log(`collections: ${JSON.stringify(listed)}`);
} finally {
  await client.close();
}
