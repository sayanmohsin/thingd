## [0.34.0](https://github.com/sayanmohsin/thingd/compare/v0.33.2...v0.34.0) (2026-06-23)

### Features

* add thingd backup CLI command using VACUUM INTO ([1e908cf](https://github.com/sayanmohsin/thingd/commit/1e908cf26099d92023e93858c37330d503f6df45))
* startup integrity check via PRAGMA quick_check ([fc451f3](https://github.com/sayanmohsin/thingd/commit/fc451f356cdadd046b6424dc5a11e3c357b7cce2))
* WAL checkpoint management ([b9b2edc](https://github.com/sayanmohsin/thingd/commit/b9b2edc45d7e92f6cab397c909578911e4d805cb))

### Bug Fixes

* error sanitization with production mode ([6744d69](https://github.com/sayanmohsin/thingd/commit/6744d69db3bffe65e2fbe43d73aaf38d0318dd03))
* input validation hardening and CORS lockdown ([e530256](https://github.com/sayanmohsin/thingd/commit/e5302563773ede7aaf4d820f0be97bdeb6e7ba95))

## [0.33.2](https://github.com/sayanmohsin/thingd/compare/v0.33.1...v0.33.2) (2026-06-22)

### Performance Improvements

* add benchmarks, sidecar tests, and optimize mutex type ([7e3a690](https://github.com/sayanmohsin/thingd/commit/7e3a690f69eb545ebb892888fc1d68ac3c73a2cc))
* optimize SQLite upsert and batch delete ([10867c6](https://github.com/sayanmohsin/thingd/commit/10867c60873765234d306b19dd5cdaadab90ff3d))

## [0.33.1](https://github.com/sayanmohsin/thingd/compare/v0.33.0...v0.33.1) (2026-06-22)

### Bug Fixes

* resolve all 24 audit issues across 4 phases ([1de3903](https://github.com/sayanmohsin/thingd/commit/1de3903efc521a1d65dcd8a5b5261e18aaef3342))

## [0.33.0](https://github.com/sayanmohsin/thingd/compare/v0.32.2...v0.33.0) (2026-06-22)

### Features

* add thingd-server Rust sidecar with MCP + REST + cluster ([24bd130](https://github.com/sayanmohsin/thingd/commit/24bd1300800496eccee79ea5cf08491398b19693))

## [0.32.2](https://github.com/sayanmohsin/thingd/compare/v0.32.1...v0.32.2) (2026-06-22)

### Bug Fixes

* configure npm auth for CI publish, remove OIDC provenance ([25338b1](https://github.com/sayanmohsin/thingd/commit/25338b1bd637e842017a46ad289247ee3185f57f))

### Performance Improvements

* skip Rust compilation in Docker, use prebuilt native binaries ([696320c](https://github.com/sayanmohsin/thingd/commit/696320cabb96b91da251347c470a8fb82debcd40))

## [0.32.1](https://github.com/sayanmohsin/thingd/compare/v0.32.0...v0.32.1) (2026-06-22)

### Bug Fixes

* update stale package references across codebase and docs ([1829297](https://github.com/sayanmohsin/thingd/commit/1829297be26dac10b6f259a1c32a22ba95c05dcc))

## [0.32.0](https://github.com/sayanmohsin/thingd/compare/v0.31.0...v0.32.0) (2026-06-21)

### Features

* add REST API for app SDKs ([215609a](https://github.com/sayanmohsin/thingd/commit/215609a4657cae198947f101784cfc0c5e360dd3))
* expose batch, sort/filter, links CLI — complete SDK surface coverage ([f350dda](https://github.com/sayanmohsin/thingd/commit/f350dda22cac5f645da11d07f6baf14858b26458))

### Bug Fixes

* update package smoke test to use @thingd/sdk ([671f26a](https://github.com/sayanmohsin/thingd/commit/671f26adae55851e7c673fc614166f97f9a2f7a9))

## [0.31.0](https://github.com/sayanmohsin/thingd/compare/v0.30.0...v0.31.0) (2026-06-20)

### Features

* add delete_last_event and delete_stream to EventLog trait ([8112320](https://github.com/sayanmohsin/thingd/commit/8112320a14f02960ac335778b6213afc18183b39)), closes [#35](https://github.com/sayanmohsin/thingd/issues/35) [#36](https://github.com/sayanmohsin/thingd/issues/36) [#37](https://github.com/sayanmohsin/thingd/issues/37)

## [0.30.0](https://github.com/sayanmohsin/thingd/compare/v0.29.0...v0.30.0) (2026-06-20)

### Features

* add graph links to SDK + MCP server (5 new tools) ([96d1e4b](https://github.com/sayanmohsin/thingd/commit/96d1e4bf4681f0c1d3b44485b05f11c646413f7d))

## [0.29.0](https://github.com/sayanmohsin/thingd/compare/v0.28.0...v0.29.0) (2026-06-19)

### Features

* add FLIP, fly, fade animations + highlight flash to dashboard lists ([87ae28e](https://github.com/sayanmohsin/thingd/commit/87ae28e3313dfcb521adf9a43a3fc1d812985f96))

## [0.28.0](https://github.com/sayanmohsin/thingd/compare/v0.27.0...v0.28.0) (2026-06-19)

### Features

* add sort, delete_objects_batch, put_object_with_options (skip FTS) ([e6e8d74](https://github.com/sayanmohsin/thingd/commit/e6e8d748cfe9c96788cc67630a9c749fdcd6de51))

## [0.27.0](https://github.com/sayanmohsin/thingd/compare/v0.26.0...v0.27.0) (2026-06-18)

### Features

* add search, count, and delete benchmarks to storage_bench ([e15bfcd](https://github.com/sayanmohsin/thingd/commit/e15bfcd2e5149b933be3e0304814ca8362add42d))

## [0.26.0](https://github.com/sayanmohsin/thingd/compare/v0.25.1...v0.26.0) (2026-06-18)

### Features

* **sdk:** add auto-reconnect to CloudThingStore on transport drop ([fb43aaa](https://github.com/sayanmohsin/thingd/commit/fb43aaa79fdec433258ce1f5ed9bea5f8377bd3e)), closes [#30](https://github.com/sayanmohsin/thingd/issues/30)

### Bug Fixes

* create_link test API, redundant closure, doc backticks ([d6fba59](https://github.com/sayanmohsin/thingd/commit/d6fba595830505eaa96ec5c0bdf6bfa390f56e32))
* **engine:** use monotonic counter for link IDs in MemoryEngine ([a428e2f](https://github.com/sayanmohsin/thingd/commit/a428e2fabee4264db545f1af7ec4102f9006b5b7)), closes [#22](https://github.com/sayanmohsin/thingd/issues/22)
* link ID dedup, list_objects filter/limit/offset, search limit consistency ([0bf9ff5](https://github.com/sayanmohsin/thingd/commit/0bf9ff5f340972a54b59c9d8206d7dcf4eafa695))
* **mcp:** fix hardcoded McpServer version and document per-request lifecycle ([39dbbb5](https://github.com/sayanmohsin/thingd/commit/39dbbb5c74f2a37f7273298cbe3adcec79598d68)), closes [#24](https://github.com/sayanmohsin/thingd/issues/24)
* **sdk:** remove hardcoded default search limit of 10 in InMemoryThingStore ([37042f7](https://github.com/sayanmohsin/thingd/commit/37042f78bf0de4d717b8caf21176876bc0fd38a7)), closes [#28](https://github.com/sayanmohsin/thingd/issues/28)
* **sdk:** stop hardcoding CloudThingStore client version as 0.1.0 ([f5afd5b](https://github.com/sayanmohsin/thingd/commit/f5afd5bec0fdf0cd5a3b07b24127e6faa08bbb74)), closes [#29](https://github.com/sayanmohsin/thingd/issues/29)

### Performance Improvements

* **engine:** push list_objects filter/limit/offset down to SQLite ([94b839b](https://github.com/sayanmohsin/thingd/commit/94b839b02796c55c7776f8af9cf706ded068493a)), closes [#27](https://github.com/sayanmohsin/thingd/issues/27)
* **engine:** use RETURNING in append_event to eliminate extra SELECT ([31c2485](https://github.com/sayanmohsin/thingd/commit/31c2485237ae0db9396a03d93542ca1673930127)), closes [#21](https://github.com/sayanmohsin/thingd/issues/21)
* **mcp:** decouple replication lag from /healthz — cache async in runner ([c83aeeb](https://github.com/sayanmohsin/thingd/commit/c83aeebc93ccef63285d0b96fc821e738f24eef5)), closes [#26](https://github.com/sayanmohsin/thingd/issues/26)
* **mcp:** pass fromSequence to events.list in replication endpoint ([32aa6cd](https://github.com/sayanmohsin/thingd/commit/32aa6cd0f49a8d285a019ec7e77dc631322f7882)), closes [#25](https://github.com/sayanmohsin/thingd/issues/25)

## [0.25.1](https://github.com/sayanmohsin/thingd/compare/v0.25.0...v0.25.1) (2026-06-18)

### Bug Fixes

* trigger patch release for crates.io publish test ([7b8bda1](https://github.com/sayanmohsin/thingd/commit/7b8bda116453e38901a2a99e55e9523961ac3455))

## [0.25.0](https://github.com/sayanmohsin/thingd/compare/v0.24.0...v0.25.0) (2026-06-17)

### Features

* add batch APIs to N-API native binding ([5cd76d0](https://github.com/sayanmohsin/thingd/commit/5cd76d0c8ec52ad7be6c2a81b3f742f857db2e54))
* add graph links support with LinkStore trait and implementations ([c992538](https://github.com/sayanmohsin/thingd/commit/c992538810d36b13c39c21ec3e5f87cdd69bc857))
* **core:** add CSV/JSON file connector with schema inference (Phase 10) ([5f89c93](https://github.com/sayanmohsin/thingd/commit/5f89c938d57362902f961b488281c63a0dda98d1))

### Bug Fixes

* apply cargo fmt formatting ([a9486c3](https://github.com/sayanmohsin/thingd/commit/a9486c32e7766cad534944264185ae555f6b08fe))
* resolve clippy warnings and duplicate dependency lint ([0f1d4b4](https://github.com/sayanmohsin/thingd/commit/0f1d4b4aff1d3ee9f391dbac1fce8492025f51b2))
* use parameterized query for link type filter in get_neighbors ([87c2f57](https://github.com/sayanmohsin/thingd/commit/87c2f5727ee2789c3a3913441d6bd5a6791c7cb6))

### Performance Improvements

* **core:** add batch APIs and optimize queue_claim_ack ([e545684](https://github.com/sayanmohsin/thingd/commit/e54568430550128adfdeac2936832528a7375fae))
* defer FTS index updates in put_objects_batch ([8d58790](https://github.com/sayanmohsin/thingd/commit/8d58790b5d14756c537100fa261a1afa9c2eb2f6))
* remove object.clone() in put_objects_batch ([8956355](https://github.com/sayanmohsin/thingd/commit/89563555f79110992782cdb064c9ca3e63168017))
* use RETURNING clause to eliminate timestamp read-back round-trip ([b4d6772](https://github.com/sayanmohsin/thingd/commit/b4d6772fa767ab5b5d6c0090eca8c78362322d3f))

## [0.24.0](https://github.com/sayanmohsin/thingd/compare/v0.23.0...v0.24.0) (2026-06-16)

### Features

* **sdk:** add apiKey as alias for authToken in ThingDOpenOptions ([#10](https://github.com/sayanmohsin/thingd/issues/10)) ([32ce660](https://github.com/sayanmohsin/thingd/commit/32ce660679c0e68ee8b417a45233da6d2ae6c633))

### Bug Fixes

* **mcp:** expand tool descriptions for better LLM tool use ([#8](https://github.com/sayanmohsin/thingd/issues/8)) ([bec0ef0](https://github.com/sayanmohsin/thingd/commit/bec0ef06c3e530fc26e909aa990629dabef5d590))

## [0.23.0](https://github.com/sayanmohsin/thingd/compare/v0.22.0...v0.23.0) (2026-06-15)

### Features

* add searchObjects() convenience method ([e7bab4e](https://github.com/sayanmohsin/thingd/commit/e7bab4e0fe1d465261608f278f6c62555175e345)), closes [#7](https://github.com/sayanmohsin/thingd/issues/7)

## [0.22.0](https://github.com/sayanmohsin/thingd/compare/v0.21.3...v0.22.0) (2026-06-14)

### Features

* add filter and pagination support to listObjects ([c6748fd](https://github.com/sayanmohsin/thingd/commit/c6748fd8cf01eb0a1336b6cc9d98b053410af9f4)), closes [#2](https://github.com/sayanmohsin/thingd/issues/2) [#3](https://github.com/sayanmohsin/thingd/issues/3)
* add generic type params to get, listObjects, and listEvents ([9922336](https://github.com/sayanmohsin/thingd/commit/9922336a5306eb5415da09ebd8cc54937bf61554)), closes [#5](https://github.com/sayanmohsin/thingd/issues/5)
* auto-generate CHANGELOG.md and release notes on publish ([d3c80d8](https://github.com/sayanmohsin/thingd/commit/d3c80d855b4fc64452a56e2ca2868ea0b09b081c))
* export ThingDConnection interface for type-safe DI ([d28a071](https://github.com/sayanmohsin/thingd/commit/d28a0710a637c98a12ede1eab3785d8da54f0639))
* implement static-config leader failover ([1ec8774](https://github.com/sayanmohsin/thingd/commit/1ec87744f944f6d92e581e42d142e6d7249c935e))
