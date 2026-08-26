import { createSalesClient } from "./mcp-client.mjs";

const { client, transport } = createSalesClient("query");

try {
  await client.connect(transport);
  const result = await client.callTool({
    name: 'thing_objects_list',
    arguments: { collection: 'orders' },
  });
  console.log(result.content[0].text);
} finally {
  await client.close();
}
