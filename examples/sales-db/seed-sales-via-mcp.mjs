import { readFile } from 'node:fs/promises';
import { createSalesClient } from "./mcp-client.mjs";

const datasets = [
  { collection: 'customers', file: 'customers.json' },
  { collection: 'products', file: 'products.json' },
  { collection: 'orders', file: 'orders.json' },
  { collection: 'order_items', file: 'order_items.json' },
];

const { client, transport } = createSalesClient("thingd-sales-seed");

try {
  await client.connect(transport);
  for (const dataset of datasets) {
    const raw = await readFile(new URL(dataset.file, import.meta.url), "utf8");
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
