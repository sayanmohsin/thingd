import { createSalesClient } from "./mcp-client.mjs";

const { client, transport } = createSalesClient("thingd-sales-query");

try {
  await client.connect(transport);
  const result = await client.callTool({
    name: 'thing_objects_list',
    arguments: { collection: 'orders' },
  });
  const orders = JSON.parse(result.content?.[0]?.text || '[]');
  const totalSales = orders.reduce((sum, order) => sum + Number(order.total || 0), 0);
  console.log(JSON.stringify({ orderCount: orders.length, totalSales }, null, 2));
} finally {
  await client.close();
}
