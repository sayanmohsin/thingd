Title: Add ACLs and rate limits to MCP endpoints

Description:
Expose access controls and rate limiting on MCP endpoints to protect against abusive clients and to support multi-tenant deployments.

Suggested features:
- Per-client API keys with scopes (read/write/admin).
- Per-tenant rate limits and global throttling.
- Audit logs for privileged actions.

Tests:
- Verify unauthorized requests are rejected; rate limits enforced; admin actions audited.

Priority: High
Labels: security, infra
