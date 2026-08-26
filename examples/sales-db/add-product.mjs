import { createSalesClient } from "./mcp-client.mjs";

const { client, transport } = createSalesClient("add-product");

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
