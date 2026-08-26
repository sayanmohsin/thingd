import { createSalesClient } from "./mcp-client.mjs";

const { client, transport } = createSalesClient("weekly-sales");

function getWeekStart(dateStr) {
  const date = new Date(dateStr);
  const day = date.getDay();
  const diff = date.getDate() - day;
  const weekStart = new Date(date.setDate(diff));
  return weekStart.toISOString().split('T')[0];
}

try {
  await client.connect(transport);
  const result = await client.callTool({
    name: 'thing_objects_list',
    arguments: { collection: 'orders' },
  });
  const orders = JSON.parse(result.content[0].text);
  
  const weeklySales = {};
  orders.forEach((order) => {
    const week = getWeekStart(order.orderDate);
    if (!weeklySales[week]) {
      weeklySales[week] = { total: 0, count: 0 };
    }
    weeklySales[week].total += order.total;
    weeklySales[week].count += 1;
  });
  
  console.log(JSON.stringify(weeklySales, null, 2));
} finally {
  await client.close();
}
