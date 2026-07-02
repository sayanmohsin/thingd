import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';

const mcpUrl = 'https://api.thingd.cloud/mcp/proj_YMEkZYgKEjMsHuZt/dkp';
const authToken = 'md_test_9372dd932b8a1b2d6282_8LdW-7eSNHpL40HJ2n0vPZau_gCSFCeQ';

const client = new Client({ name: 'add-product', version: '0.1.0' });
const transport = new StreamableHTTPClientTransport(new URL(mcpUrl), {
  requestInit: { headers: { Authorization: `Bearer ${authToken}` } },
});

const newProduct = {
  id: 'prod-006',
  name: 'Enterprise Support',
  category: 'Support',
  price: 499.99,
};

try {
  await client.connect(transport);
  const result = await client.callTool({
    name: 'thing_put',
    arguments: {
      collection: 'products',
      object: newProduct,
    },
  });
  console.log(result.content[0].text);
} finally {
  await client.close();
}
