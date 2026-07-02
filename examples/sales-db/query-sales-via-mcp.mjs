import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';

const mcpUrl = 'https://api.thingd.cloud/mcp/proj_YMEkZYgKEjMsHuZt/dkp';
const authToken = 'md_test_9372dd932b8a1b2d6282_8LdW-7eSNHpL40HJ2n0vPZau_gCSFCeQ';

const client = new Client({ name: 'thingd-sales-query', version: '0.1.0' });
const transport = new StreamableHTTPClientTransport(new URL(mcpUrl), {
  requestInit: {
    headers: {
      Authorization: `Bearer ${authToken}`,
    },
  },
});

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
