# Sample Sales Database

This example creates a small, realistic sales dataset that can be imported into ThingD or used as a reference for MCP-based data workflows.

## Contents

- customers.json
- products.json
- orders.json
- order_items.json
- seed.sh

## Quick start

Install the example dependencies from the repository root:

```bash
pnpm install
```

Run:

```bash
cd examples/sales-db
./seed.sh
```

This will create JSON files in the current directory if they do not already exist.

To run the MCP scripts against a Thingd instance, provide the endpoint and
token through the environment. Never commit credentials to an example:

```bash
THINGD_MCP_URL="https://your-thingd-host/mcp/your-project/your-instance" \
THINGD_AUTH_TOKEN="your-token" \
node query-sales-via-mcp.mjs
```
