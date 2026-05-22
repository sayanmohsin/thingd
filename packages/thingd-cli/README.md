# thingd-cli

Admin and operator CLI for `thingd`.

This package provides the `thingd` binary. It uses the public
`thingd` SDK for store access and can connect to a local store or
a remote sidecar through `THINGD_URL`.

## Build And Test

```bash
pnpm --filter thingd-cli build
pnpm --filter thingd-cli test
```

## Usage

```bash
thingd status
thingd objects put decisions rust-core --text "Use Rust for the core engine."
thingd objects get decisions rust-core
thingd events append project:thingd decision.made --text "Picked the CLI shape."
thingd events list project:thingd
thingd queues push embed --payload '{"object":"docs/readme"}'
thingd queues claim embed
```

Remote sidecar mode:

```bash
THINGD_URL=http://127.0.0.1:8757
THINGD_AUTH_TOKEN=change-me
thingd status
thingd tools
```

Common options:

```txt
--url <url>          remote thingd URL. Defaults to THINGD_URL
--auth-token <tok>  remote bearer token. Defaults to THINGD_AUTH_TOKEN
--path <path>       local database path. Defaults to THINGD_PATH or :memory:
--driver <driver>   memory, native, or remote
--pretty            pretty-print JSON output
--limit <n>         result limit for search and list commands
```

The first CLI version intentionally emits JSON so it is easy to use from shell
scripts, tests, and AI-agent workflows.

