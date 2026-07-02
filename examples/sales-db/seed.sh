#!/usr/bin/env bash
set -euo pipefail

mkdir -p "$(dirname "$0")"

cat > "$(dirname "$0")/customers.json" <<'EOF'
[
  {"id": "cust-001", "name": "Ava Patel", "email": "ava@example.com", "segment": "Enterprise", "country": "US"},
  {"id": "cust-002", "name": "Liam Chen", "email": "liam@example.com", "segment": "SMB", "country": "CA"},
  {"id": "cust-003", "name": "Sofia Alvarez", "email": "sofia@example.com", "segment": "Mid-Market", "country": "US"},
  {"id": "cust-004", "name": "Noah Kim", "email": "noah@example.com", "segment": "SMB", "country": "AU"},
  {"id": "cust-005", "name": "Mina Rossi", "email": "mina@example.com", "segment": "Enterprise", "country": "DE"}
]
EOF

cat > "$(dirname "$0")/products.json" <<'EOF'
[
  {"id": "prod-001", "name": "Starter Plan", "category": "Software", "price": 49.99},
  {"id": "prod-002", "name": "Pro Analytics", "category": "Analytics", "price": 129.0},
  {"id": "prod-003", "name": "Team Workspace", "category": "Software", "price": 89.5},
  {"id": "prod-004", "name": "AI Assistant", "category": "AI", "price": 199.99},
  {"id": "prod-005", "name": "Data Sync", "category": "Integration", "price": 79.0}
]
EOF

cat > "$(dirname "$0")/orders.json" <<'EOF'
[
  {"id": "order-001", "customerId": "cust-001", "orderDate": "2026-01-10", "status": "Paid", "total": 179.49},
  {"id": "order-002", "customerId": "cust-003", "orderDate": "2026-01-12", "status": "Pending", "total": 89.5},
  {"id": "order-003", "customerId": "cust-002", "orderDate": "2026-02-03", "status": "Paid", "total": 329.0},
  {"id": "order-004", "customerId": "cust-005", "orderDate": "2026-02-14", "status": "Paid", "total": 249.99},
  {"id": "order-005", "customerId": "cust-004", "orderDate": "2026-03-01", "status": "Shipped", "total": 128.99}
]
EOF

cat > "$(dirname "$0")/order_items.json" <<'EOF'
[
  {"id": "item-001", "orderId": "order-001", "productId": "prod-001", "quantity": 1, "unitPrice": 49.99},
  {"id": "item-002", "orderId": "order-001", "productId": "prod-002", "quantity": 1, "unitPrice": 129.5},
  {"id": "item-003", "orderId": "order-002", "productId": "prod-003", "quantity": 1, "unitPrice": 89.5},
  {"id": "item-004", "orderId": "order-003", "productId": "prod-002", "quantity": 2, "unitPrice": 129.0},
  {"id": "item-005", "orderId": "order-004", "productId": "prod-004", "quantity": 1, "unitPrice": 199.99},
  {"id": "item-006", "orderId": "order-004", "productId": "prod-005", "quantity": 1, "unitPrice": 79.0},
  {"id": "item-007", "orderId": "order-005", "productId": "prod-001", "quantity": 1, "unitPrice": 49.99},
  {"id": "item-008", "orderId": "order-005", "productId": "prod-005", "quantity": 1, "unitPrice": 79.0}
]
EOF

echo "Sample sales data created in $(dirname "$0")"
