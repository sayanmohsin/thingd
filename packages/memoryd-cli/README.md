# @sayanmohsin/memoryd-cli

Admin and operator CLI for `memoryd`.

This package provides the `memoryd` binary. It uses the public
`@sayanmohsin/memoryd` SDK for store access and can connect to a local store or
a remote sidecar through `MEMORYD_URL`.

## Build And Test

```bash
pnpm --filter @sayanmohsin/memoryd-cli build
pnpm --filter @sayanmohsin/memoryd-cli test
```

## Usage

```bash
memoryd status
memoryd objects put decisions rust-core --text "Use Rust for the core engine."
memoryd objects get decisions rust-core
memoryd events append project:memoryd decision.made --text "Picked the CLI shape."
memoryd events list project:memoryd
memoryd queues push embed --payload '{"object":"docs/readme"}'
memoryd queues claim embed
```

Remote sidecar mode:

```bash
MEMORYD_URL=http://127.0.0.1:8757
MEMORYD_AUTH_TOKEN=change-me
memoryd status
memoryd tools
```

Common options:

```txt
--url <url>          remote memoryd URL. Defaults to MEMORYD_URL
--auth-token <tok>  remote bearer token. Defaults to MEMORYD_AUTH_TOKEN
--path <path>       local database path. Defaults to MEMORYD_PATH or :memory:
--driver <driver>   memory, native, or remote
--pretty            pretty-print JSON output
--limit <n>         result limit for search and list commands
```

The first CLI version intentionally emits JSON so it is easy to use from shell
scripts, tests, and AI-agent workflows.

