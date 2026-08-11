## [0.72.0](https://github.com/sayanmohsin/thingd/compare/v0.71.0...v0.72.0) (2026-08-02)

### Features

- align docs metadata and return imported objects (#69) (4084502)
- add hosted app backend contract and client (#70) (5d5e307)

### Bug Fixes

- tolerate missing release doc version examples (d8df795)
- prepare protected semantic release PRs (5982c3c)
- make metadata generation cwd independent (#74) (d8eac3f)
- server: harden security boundaries (0961ffe)
- complete public SDK type exports (8a8889a)

## [0.79.1](https://github.com/sayanmohsin/thingd/compare/thingd-v0.79.0...thingd-v0.79.1) (2026-08-11)


### Bug Fixes

* publish complete branch contents ([#137](https://github.com/sayanmohsin/thingd/issues/137)) ([945748c](https://github.com/sayanmohsin/thingd/commit/945748cb04f2b3abac4ed837a777ae02cecd1766))

## [0.79.0](https://github.com/sayanmohsin/thingd/compare/thingd-v0.78.0...thingd-v0.79.0) (2026-08-10)


### Features

* add schema metadata writes and native replication ([#133](https://github.com/sayanmohsin/thingd/issues/133)) ([0bd6d1c](https://github.com/sayanmohsin/thingd/commit/0bd6d1ce5ffe6f796278e538ca800f91fad7b125))

## [0.78.0](https://github.com/sayanmohsin/thingd/compare/thingd-v0.77.3...thingd-v0.78.0) (2026-08-10)


### Features

* add native Thingd replication ([#130](https://github.com/sayanmohsin/thingd/issues/130)) ([f843f05](https://github.com/sayanmohsin/thingd/commit/f843f05e919625295bb47da27fe50198dd25b7a5))
* add native Thingd replication ([#132](https://github.com/sayanmohsin/thingd/issues/132)) ([f2a6292](https://github.com/sayanmohsin/thingd/commit/f2a6292a44464c3dfe496bb9eaab3a1e6f7e9f90))

## [0.77.3](https://github.com/sayanmohsin/thingd/compare/thingd-v0.77.2...thingd-v0.77.3) (2026-08-09)


### Bug Fixes

* wait for npm publication propagation ([#126](https://github.com/sayanmohsin/thingd/issues/126)) ([7fc23a5](https://github.com/sayanmohsin/thingd/commit/7fc23a5df29ac733284b4ca1576e9b01e5b6ba31))

## [0.77.2](https://github.com/sayanmohsin/thingd/compare/thingd-v0.77.1...thingd-v0.77.2) (2026-08-09)


### Bug Fixes

* avoid registry resolution during publish ([#124](https://github.com/sayanmohsin/thingd/issues/124)) ([f676907](https://github.com/sayanmohsin/thingd/commit/f676907327df2a236dca493362800c7ddc187b15))

## [0.77.1](https://github.com/sayanmohsin/thingd/compare/thingd-v0.77.0...thingd-v0.77.1) (2026-08-09)


### Bug Fixes

* harden release publish metadata ([#122](https://github.com/sayanmohsin/thingd/issues/122)) ([8e70269](https://github.com/sayanmohsin/thingd/commit/8e7026994ef615570bb45d0d15bc48bfd9c8493d))

## [0.77.0](https://github.com/sayanmohsin/thingd/compare/thingd-v0.76.0...thingd-v0.77.0) (2026-08-09)


### Features

* add optional .thingd schemas and migrations ([#120](https://github.com/sayanmohsin/thingd/issues/120)) ([41b044f](https://github.com/sayanmohsin/thingd/commit/41b044f60115186f36e952c37b08422d0e7c7bb6))
* harden provider-neutral replication and persistent encryption ([#118](https://github.com/sayanmohsin/thingd/issues/118)) ([f5c41c1](https://github.com/sayanmohsin/thingd/commit/f5c41c1eabf32a140bd67b51aa4cbb9f22235a48))

## [0.76.0](https://github.com/sayanmohsin/thingd/compare/thingd-v0.75.0...thingd-v0.76.0) (2026-08-07)


### Features

* **storage:** complete encrypted persistence integration ([#114](https://github.com/sayanmohsin/thingd/issues/114)) ([caba1b5](https://github.com/sayanmohsin/thingd/commit/caba1b5a2087680f9d9a981ae4438489232d2c5c))


### Bug Fixes

* **storage:** rebuild incompatible legacy search indexes ([#116](https://github.com/sayanmohsin/thingd/issues/116)) ([de88f56](https://github.com/sayanmohsin/thingd/commit/de88f562d9350cb1c05325c325285e2cf131014a))

## [0.75.0](https://github.com/sayanmohsin/thingd/compare/thingd-v0.74.3...thingd-v0.75.0) (2026-08-07)


### Features

* **storage:** add pluggable encrypted persistence codec ([#112](https://github.com/sayanmohsin/thingd/issues/112)) ([30593cf](https://github.com/sayanmohsin/thingd/commit/30593cf89b2ecfb5a13ef4a5000f981826366647))

## [0.74.3](https://github.com/sayanmohsin/thingd/compare/thingd-v0.74.2...thingd-v0.74.3) (2026-08-04)


### Bug Fixes

* harden connector sidecar execution ([#106](https://github.com/sayanmohsin/thingd/issues/106)) ([054876e](https://github.com/sayanmohsin/thingd/commit/054876eec8582dee5173a483f268779e8d525646))
* resolve 10 CodeQL security alerts ([#108](https://github.com/sayanmohsin/thingd/issues/108)) ([1781864](https://github.com/sayanmohsin/thingd/commit/17818644d0279189723df349e7eb3d4a61ae39a8))

## [0.74.2](https://github.com/sayanmohsin/thingd/compare/thingd-v0.74.1...thingd-v0.74.2) (2026-08-03)


### Bug Fixes

* authenticate npm package publishing ([#104](https://github.com/sayanmohsin/thingd/issues/104)) ([b394bf3](https://github.com/sayanmohsin/thingd/commit/b394bf31e5c1c556e153c25b3b487cb32c8d3b62))

## [0.74.1](https://github.com/sayanmohsin/thingd/compare/thingd-v0.74.0...thingd-v0.74.1) (2026-08-03)


### Bug Fixes

* support pnpm publish artifact validation ([#102](https://github.com/sayanmohsin/thingd/issues/102)) ([e557950](https://github.com/sayanmohsin/thingd/commit/e557950d99e08b6765d284a16afc942789b2e22f))

## [0.74.0](https://github.com/sayanmohsin/thingd/compare/thingd-v0.73.1...thingd-v0.74.0) (2026-08-03)


### Features

* add scalable tenant JWT authentication ([#100](https://github.com/sayanmohsin/thingd/issues/100)) ([cb6ffca](https://github.com/sayanmohsin/thingd/commit/cb6ffcaaf50bdf3bfeceadf6cdbadc7bbe6db9e8))

## [0.73.1](https://github.com/sayanmohsin/thingd/compare/thingd-v0.73.0...thingd-v0.73.1) (2026-08-02)


### Bug Fixes

* use release tag output in publishing workflow ([#98](https://github.com/sayanmohsin/thingd/issues/98)) ([3b46b21](https://github.com/sayanmohsin/thingd/commit/3b46b21d9e87e13beadba5903ed8c9c12ebfd116))

## [0.73.0](https://github.com/sayanmohsin/thingd/compare/thingd-v0.72.0...thingd-v0.73.0) (2026-08-02)


### Features

* add aggregate queries and schema reflection ([f3e5b24](https://github.com/sayanmohsin/thingd/commit/f3e5b24a41f9fb3d81ea824e157572f0b7ad8b95))
* add collection bar chart and capacity gauge to TUI metrics view ([23f0f4e](https://github.com/sayanmohsin/thingd/commit/23f0f4e1b454ef9df3a8c2b4782577ca28155546))
* add event payload, queue job payload, and dead job retry to TUI ([6abb057](https://github.com/sayanmohsin/thingd/commit/6abb05763f259dd3f13e57fbfc417c2be9dc3331))
* add export, import, and dashboard launch to TUI ([62420df](https://github.com/sayanmohsin/thingd/commit/62420df24e02794bbc68acc3d1c5aef97ef84c59))
* add fjall.rs unit tests (38), close() support, Tantivy deletion fix ([5a35cb8](https://github.com/sayanmohsin/thingd/commit/5a35cb8f47fd68dd7d7572e837f0e94f31c678c6))
* add functional indexes for json_extract filter queries ([a4673a8](https://github.com/sayanmohsin/thingd/commit/a4673a8aa6c77f5b8e03b48b7a08ea881fc77555)), closes [#53](https://github.com/sayanmohsin/thingd/issues/53)
* add hosted app backend contract and client ([#70](https://github.com/sayanmohsin/thingd/issues/70)) ([5d5e307](https://github.com/sayanmohsin/thingd/commit/5d5e30755d16fec388dad316cb935cf7ff101cc8))
* add index on objects.updated_at for sort-by-recency queries ([222da8f](https://github.com/sayanmohsin/thingd/commit/222da8fe42c4a02946ce75d5914cd642beb7fba0)), closes [#55](https://github.com/sayanmohsin/thingd/issues/55)
* add keyboard help overlay, breadcrumbs, and status bar to TUI ([7d65e6d](https://github.com/sayanmohsin/thingd/commit/7d65e6d40a06a36c8bfa98e29bb01907bb33618e))
* add links browsing with CRUD and neighbors viewer to TUI ([8bfc15c](https://github.com/sayanmohsin/thingd/commit/8bfc15cde36c691e9d345f0116af0f10328994db))
* add NLQ dashboard tab with schema browser and LLM-powered query ([1573893](https://github.com/sayanmohsin/thingd/commit/1573893fb4da3c9d1ae5d4879c28799be1978403))
* add NLQ LLM client to sidecar with REST and MCP tools ([debfd4b](https://github.com/sayanmohsin/thingd/commit/debfd4bff75834ee563573557bc63d7bad02138d))
* add object sort/filter/pagination and batch operations to TUI ([e5c364a](https://github.com/sayanmohsin/thingd/commit/e5c364ac9074017793b45e91c92153cadbf0935e))
* add per-collection countObjects(collection) method ([b015f93](https://github.com/sayanmohsin/thingd/commit/b015f93ea2ca16d21de2b285ea18d3d67b0c3171)), closes [#57](https://github.com/sayanmohsin/thingd/issues/57)
* add priority support for queue jobs ([8e8b013](https://github.com/sayanmohsin/thingd/commit/8e8b013797ae6e2e95d3bb8dc31f8321dae7ce14)), closes [#59](https://github.com/sayanmohsin/thingd/issues/59)
* add schema, aggregate, timeseries, and NLQ viewers to TUI ([bde1e30](https://github.com/sayanmohsin/thingd/commit/bde1e300681ae60697e562daa0769a7a4f18d4ff))
* add shell completions command to CLI ([2033bd1](https://github.com/sayanmohsin/thingd/commit/2033bd124d6648f9d2fa9ecf3fb9b22702343d76))
* add thingd backup CLI command using VACUUM INTO ([1e908cf](https://github.com/sayanmohsin/thingd/commit/1e908cf26099d92023e93858c37330d503f6df45))
* add toast notifications and loading indicator to TUI ([ff15cba](https://github.com/sayanmohsin/thingd/commit/ff15cbad25408bebddf4b6764b0d4c56c2c4dcf8))
* align docs metadata and return imported objects ([#69](https://github.com/sayanmohsin/thingd/issues/69)) ([4084502](https://github.com/sayanmohsin/thingd/commit/40845020eee8f4fefba78c545f8e6f9ae3833f24))
* atomic restore and backup-before-migration ([fd03859](https://github.com/sayanmohsin/thingd/commit/fd038591f5814c73135507b4e9fc022cb3ab48c4))
* centralize MCP tool count in constants.ts, add to VitePress theme config, update docs ([20eb79b](https://github.com/sayanmohsin/thingd/commit/20eb79b2adcdfde730b69fd33e2d848b977a7cfa))
* CLI db subcommand, dashboard health tab, security/operations docs ([7ae5970](https://github.com/sayanmohsin/thingd/commit/7ae597042fab33d08b4fdb7d599374e09e1044a8))
* **cli,cloud:** issue API key after login so REST/MCP data plane works ([c072963](https://github.com/sayanmohsin/thingd/commit/c0729638c31525127c3c4b10afc9b14505c3ddcb))
* **cli:** add organization subcommands to cloud module ([e9fc5cb](https://github.com/sayanmohsin/thingd/commit/e9fc5cb048c4c6967aafa8e6e38ec8de7db72033))
* **cli:** add token cleanup subcommand ([8a7055f](https://github.com/sayanmohsin/thingd/commit/8a7055f24a9d77c92fcd551c17611ca98347bc6c))
* **cli:** interactive instance picker for cloud login + mock tests ([3e2859f](https://github.com/sayanmohsin/thingd/commit/3e2859f29e524ea90dab2bbf51e53bf892bbf0aa))
* **cli:** use cloud login credentials for TUI, dashboard, and CLI ([13b2e7f](https://github.com/sayanmohsin/thingd/commit/13b2e7fe1c510e8d4a19d013760a59d5bf7806e6))
* **cli:** user token auth system ([ef13509](https://github.com/sayanmohsin/thingd/commit/ef1350999bd038696f2252634846eb05707f9be7))
* cloud CLI commands with login, project, instance, api-key management ([828e979](https://github.com/sayanmohsin/thingd/commit/828e9790fa11d26849047afea5643dc25092b8ef))
* **connectors:** add list_tables() to Connector trait ([38a7287](https://github.com/sayanmohsin/thingd/commit/38a7287e5d993e3cf2cbd26a7fb6570808edc858))
* **connectors:** add ping endpoint and test-connection UI ([236020f](https://github.com/sayanmohsin/thingd/commit/236020f1d5577b8a3cc745a2ae598852efeae516))
* **connectors:** Postgres and MySQL connectors with streaming PullStream ([8cdc45d](https://github.com/sayanmohsin/thingd/commit/8cdc45dd274a60f0099f5cc76377c3ad027794fa))
* **connectors:** wire Postgres/MySQL connectors through all layers ([4cc1076](https://github.com/sayanmohsin/thingd/commit/4cc1076be3b9e2e05f146d894f384cb8ef0cb4a8))
* **engine:** add `since` timestamp filter to events.list() API ([cb377db](https://github.com/sayanmohsin/thingd/commit/cb377db7607d278d329885f2c02fd0036b47e1d4)), closes [#51](https://github.com/sayanmohsin/thingd/issues/51)
* **engine:** add event idempotency via idempotencyKey ([cf77768](https://github.com/sayanmohsin/thingd/commit/cf77768e9ccd0da022918daf6a1499e459ed9eb3)), closes [#49](https://github.com/sayanmohsin/thingd/issues/49)
* **engine:** add getBatch / getMany for batch object reads ([935e122](https://github.com/sayanmohsin/thingd/commit/935e12257cf8954694e353578c3141faf867a7fc)), closes [#52](https://github.com/sayanmohsin/thingd/issues/52)
* **engine:** add optimistic locking / CAS support to put() ([2cc69f1](https://github.com/sayanmohsin/thingd/commit/2cc69f19bb0e54ca81d6efe23a264797cf81d14c)), closes [#43](https://github.com/sayanmohsin/thingd/issues/43)
* **engine:** add POST /admin/clear-default-db + EnginePool.clear_default_engine() ([ea56f85](https://github.com/sayanmohsin/thingd/commit/ea56f859f3600b77b32112f6677b28ca44a1aae6))
* **engine:** add vector field, VectorStore trait, and cosine-similarity search ([71e1e9d](https://github.com/sayanmohsin/thingd/commit/71e1e9d529e356689c09889f53479c5bbeab9681)), closes [#60](https://github.com/sayanmohsin/thingd/issues/60)
* expose connector table discovery ([0dc9ab6](https://github.com/sayanmohsin/thingd/commit/0dc9ab620f23e514c4d7d20a20caea6294e966e2))
* harden Fjall engine against restart, index, and consistency failures ([da6f84e](https://github.com/sayanmohsin/thingd/commit/da6f84e42aa4a2b8a5b191da9abcd53982416539))
* implement all remaining improvements ([29a064f](https://github.com/sayanmohsin/thingd/commit/29a064f721899c905c82b5aff29c7a7ab0d93e4f))
* make listObjectsJson async to avoid blocking event loop ([f997ac0](https://github.com/sayanmohsin/thingd/commit/f997ac0839f272816ef0e5f3d585d91596336247)), closes [#54](https://github.com/sayanmohsin/thingd/issues/54)
* make spec-driven workflow discoverable ([0494bc0](https://github.com/sayanmohsin/thingd/commit/0494bc0916634bbf5371d4058b7988bdc614c40d))
* rate limiting with token bucket middleware ([328b974](https://github.com/sayanmohsin/thingd/commit/328b97484466084f8ecf9c516715ca67a2c1f5bb))
* **sdk:** add built-in scheduler with persistence, observability, and MCP tools ([8b45423](https://github.com/sayanmohsin/thingd/commit/8b4542318b1da2197731bc77eadbcff451f9fa49))
* **server:** wire tenant isolation into all REST handlers ([74e0709](https://github.com/sayanmohsin/thingd/commit/74e0709a2a217e1e4a3a0201f432f66d2248ef3a)), closes [#61](https://github.com/sayanmohsin/thingd/issues/61)
* show clear messages when cloud config is missing in CLI/TUI ([feab607](https://github.com/sayanmohsin/thingd/commit/feab607dbce6f792a78c5684d6264b40a884f30e))
* **sidecar+sdk:** add thing_vector_search MCP tool, REST endpoint, and TypeScript SDK support ([78488d5](https://github.com/sayanmohsin/thingd/commit/78488d5b415adffe2d43a544d5357dd731ace314)), closes [#60](https://github.com/sayanmohsin/thingd/issues/60)
* **sidecar:** add /metrics endpoint with Prometheus-formatted store metrics ([9a5f15b](https://github.com/sayanmohsin/thingd/commit/9a5f15bea80c72054e1192309d583c3ef673a714)), closes [#46](https://github.com/sayanmohsin/thingd/issues/46)
* **sidecar:** implement all 27 MCP tools (was 5 stubs) ([e6f45e9](https://github.com/sayanmohsin/thingd/commit/e6f45e97fc32811f39a29e3cb3729dffbc783718))
* **skill:** add audit-after-change skill — doc cross-ref, thingd-cloud sync, test gap check ([0958e12](https://github.com/sayanmohsin/thingd/commit/0958e12818a1e3f6bbd1fcb9fae02df0907677ee))
* startup integrity check via PRAGMA quick_check ([fc451f3](https://github.com/sayanmohsin/thingd/commit/fc451f356cdadd046b6424dc5a11e3c357b7cce2))
* **storage:** replace SQLite with Fjall LSM-tree and Tantivy search ([c27722e](https://github.com/sayanmohsin/thingd/commit/c27722e865c2291d47c900f032eb8a2ab375f8ae))
* support comparison operators in listObjects filters ([1b00ad3](https://github.com/sayanmohsin/thingd/commit/1b00ad36a891c2b85b3923384192ac1eef4fd06d)), closes [#56](https://github.com/sayanmohsin/thingd/issues/56)
* support sorting by JSON body fields in listObjects ([853ae35](https://github.com/sayanmohsin/thingd/commit/853ae35fedc335bd2c9dcb759c27cd1274e412bb)), closes [#58](https://github.com/sayanmohsin/thingd/issues/58)
* **tenant:** wire TenantConfig into MCP handler for per-instance DB isolation ([e50696f](https://github.com/sayanmohsin/thingd/commit/e50696f066062ac08a76ff15af3701bf8b17c298))
* **tui:** add link count and db size sparkline to metrics ([dab7fd4](https://github.com/sayanmohsin/thingd/commit/dab7fd476095e4e45a316b310013c858e6bbf009))
* **tui:** rich sidebar labels with inline metadata ([c2554bc](https://github.com/sayanmohsin/thingd/commit/c2554bcb2f82a387c5ce27cab8c29a0298b10084))
* WAL checkpoint management ([b9b2edc](https://github.com/sayanmohsin/thingd/commit/b9b2edc45d7e92f6cab397c909578911e4d805cb))


### Bug Fixes

* add JSDoc to vectorSearch method in @thingd/client ([7c208a6](https://github.com/sayanmohsin/thingd/commit/7c208a6ae31771bd3dfceb115b032ef736907c5c))
* add links and cloud commands to CLI HELP_TEXT ([5500ebe](https://github.com/sayanmohsin/thingd/commit/5500ebe290e34d3a422752a275de444fdb931ac9))
* add migrate feature to thingd-server so Migration cfg compiles ([c7ff3d2](https://github.com/sayanmohsin/thingd/commit/c7ff3d2c726f02a31f581acb477d3c5320683ea8))
* add missing /auth/user-tokens handler to cloud login mock servers ([14b7738](https://github.com/sayanmohsin/thingd/commit/14b7738ca65f6bf99da04fafe25e82d20972860d))
* add missing colon to docs site logo ([73e7ace](https://github.com/sayanmohsin/thingd/commit/73e7ace22fbba0726f7315a3ca1d095eb29a27aa))
* add pnpm test:cli and pnpm test:rust to pre-push hook (was missing, causing CI failures) ([65a100b](https://github.com/sayanmohsin/thingd/commit/65a100b28b4bca0a352be1368e82cb175c7ae28d))
* add ref:main to all release workflow checkouts ([42593f3](https://github.com/sayanmohsin/thingd/commit/42593f321a8dad6620717a0fe7298c6e3b824d90))
* add require condition to exports for CJS compatibility ([8ad7357](https://github.com/sayanmohsin/thingd/commit/8ad7357892ac16abd446ce58de14478baf24664d))
* add Rust toolchain + native build to CLI test job ([c3ce088](https://github.com/sayanmohsin/thingd/commit/c3ce088a8bd64c307d87a3af0eeec2b2eb8064f0))
* add User-Agent to crates release lookup ([79516a8](https://github.com/sayanmohsin/thingd/commit/79516a861195454a7db855b2a2462cb2d8d8ac3e))
* add X-Tenant-Id to CORS allowed headers ([6d9b816](https://github.com/sayanmohsin/thingd/commit/6d9b816404e3b4e3ec23c66abcca814fbf3b8dc9))
* address test failures and native binding issues ([403f814](https://github.com/sayanmohsin/thingd/commit/403f814b5ff87d246fdfbc7e11d73383017b03ac))
* auto-fix biome lint errors in cloud.ts ([afaae7a](https://github.com/sayanmohsin/thingd/commit/afaae7a44eadb35c3bd084d54b2bd51d0a565326))
* await async listObjectsJson in CI inline tests ([5a9b8b1](https://github.com/sayanmohsin/thingd/commit/5a9b8b1083fa79b02e1c995f66469f6b401dd730)), closes [#54](https://github.com/sayanmohsin/thingd/issues/54)
* biome lint in docs theme (hoisted h(), import order) ([02ee53f](https://github.com/sayanmohsin/thingd/commit/02ee53f43dac043f2e00e5ec196b8433e3316cca))
* build native x64 on Intel macOS ([#93](https://github.com/sayanmohsin/thingd/issues/93)) ([91215d4](https://github.com/sayanmohsin/thingd/commit/91215d46bf462727aed5c50e5209b6f3d9242f39))
* build packages before npm publish ([#87](https://github.com/sayanmohsin/thingd/issues/87)) ([ddbca81](https://github.com/sayanmohsin/thingd/commit/ddbca813d0394b8dc3aaf51d6d98b6bf547bc982))
* build static musl binaries for Docker multi-arch image ([673c163](https://github.com/sayanmohsin/thingd/commit/673c163adb8a04cbf2bebf636e2b26aeb2876c9c))
* bump thingd version specifiers from 0.37 to 0.38 ([e3876f4](https://github.com/sayanmohsin/thingd/commit/e3876f45634c3b3844700ab1a3e7591ecf808fc0))
* cargo fmt, biome lint, and pre-existing warnings ([8acbb28](https://github.com/sayanmohsin/thingd/commit/8acbb288e832db9e723fbf0bcb908b454a4f0f97))
* cargo-deny config format for v0.18+ ([bc1cedc](https://github.com/sayanmohsin/thingd/commit/bc1cedce904c1a70e148515030eac44e49538969))
* change VitePress base from /thingd/ to / for custom domain ([3308cc3](https://github.com/sayanmohsin/thingd/commit/3308cc3c41d586ac29740aa9a4280ec4ebda7756))
* **ci:** add +crt-static to multi-arch Docker builds ([4124689](https://github.com/sayanmohsin/thingd/commit/4124689c2d319d21bffd9b3f10f8032a3fc78a96)), closes [#66](https://github.com/sayanmohsin/thingd/issues/66)
* **ci:** add fetch-tags: true to release workflow checkout steps ([a560b7d](https://github.com/sayanmohsin/thingd/commit/a560b7d8f09e04bb6e5a010fc11b0e374668b6d2))
* **ci:** bump and publish @thingd/client alongside other packages ([e482cb4](https://github.com/sayanmohsin/thingd/commit/e482cb4c1ced8f2ee668ca13f547a84306fe783b))
* **ci:** bump docker/setup-qemu-action from v3 to v4 ([768d6bf](https://github.com/sayanmohsin/thingd/commit/768d6bf426e1e829e7a9c883be72868724333679))
* **ci:** extract zig to user-writable path instead of /usr/local ([c6b2732](https://github.com/sayanmohsin/thingd/commit/c6b2732fdb3c6e777445ae7379203970c321529e))
* **ci:** install zig for cargo-zigbuild musl cross-compilation ([3bab8dc](https://github.com/sayanmohsin/thingd/commit/3bab8dce9d15d0fc68fd849ff9395f57879078e0))
* **ci:** only trigger release workflow for releasable commits (feat, fix, BREAKING CHANGE) ([6c5cec8](https://github.com/sayanmohsin/thingd/commit/6c5cec883e5c49782965dfcc99e6373fba58373c))
* **ci:** publish multi-arch Docker images (linux/amd64 + linux/arm64) ([b875363](https://github.com/sayanmohsin/thingd/commit/b87536392cc04eacbe074cb5b2aab376f1ca5684)), closes [#65](https://github.com/sayanmohsin/thingd/issues/65)
* **ci:** resolve cargo-deny and vitepress build failures ([c622de4](https://github.com/sayanmohsin/thingd/commit/c622de46162298c0081cd5ca9fc5e8421458d960))
* **cli:** cloud connect form now uses project/instance slug picker instead of raw URL ([e9f2ba9](https://github.com/sayanmohsin/thingd/commit/e9f2ba9dc0610e487746197089ab3275b644dd60))
* **cli:** cloud TUI connect now uses instance MCP URL instead of API base URL ([a9ce86f](https://github.com/sayanmohsin/thingd/commit/a9ce86f82fc40fdaf87ea11d405a4e914b2e2dd9))
* **cli:** hoist readline interface to avoid race in askQuestion ([89b1ed6](https://github.com/sayanmohsin/thingd/commit/89b1ed692f56ec0b68249550de460e3baee8290c))
* **cli:** loosen regex to match across ANSI color codes in test assertions ([139e8f6](https://github.com/sayanmohsin/thingd/commit/139e8f66f7567c7cc73c47bd68ebfe9d37b97c72))
* **cli:** only auto-connect cloud with concrete instanceUrl, not bare API url ([fa6b652](https://github.com/sayanmohsin/thingd/commit/fa6b652f26bb10e1589dbd30c0218e6d82ee62f5))
* **cli:** relabel 'MCP URL' → 'Cloud URL' in cloud connection form ([98df5d5](https://github.com/sayanmohsin/thingd/commit/98df5d55ae5f25232261a557812b67e238a6192d))
* **cli:** replace hardcoded version 0.33.0 with SDK_VERSION ([a28b562](https://github.com/sayanmohsin/thingd/commit/a28b562d3037447bdc4188054c25ba9eef67009e))
* **cli:** save cloud config before instance discovery in login flow ([42bf14c](https://github.com/sayanmohsin/thingd/commit/42bf14ce7ede7ac06dd8695b0c32b1be902f3d03))
* **cli:** skip timing-dependent cluster tests in CI ([7f35bee](https://github.com/sayanmohsin/thingd/commit/7f35beef6aaa1b27b2d741249bc94a653631e41a))
* **cli:** update 14 stale cloud API paths after thingd-cloud route refactor ([60ecf7e](https://github.com/sayanmohsin/thingd/commit/60ecf7e1ee9e7e5d90c61879caaf092316d92ed3))
* **cli:** use CloudThingStore for cloud driver, add instance picker and logout ([22a75dc](https://github.com/sayanmohsin/thingd/commit/22a75dcc9c1e0927e4515ac23f42682328f590e1))
* **cli:** writeJson for db subcommands, remove fragile internal state access in TUI ([7a7376e](https://github.com/sayanmohsin/thingd/commit/7a7376ef813acedcd399cfa1d5cfed4725f36519))
* CloudThingStore batch methods now use MCP batch tools ([d0eca83](https://github.com/sayanmohsin/thingd/commit/d0eca831000e9c393787275ff73808e25a4fa4a8))
* cluster status test port mismatch — remove stale advertiseUrl/peers ([0b4f48d](https://github.com/sayanmohsin/thingd/commit/0b4f48d0ee2d3d0f7fed844eade39896d00226df))
* complete all partial phase items ([535a8d2](https://github.com/sayanmohsin/thingd/commit/535a8d2224757dad933f7600f70a947a32a8a982))
* complete public SDK type exports ([8a8889a](https://github.com/sayanmohsin/thingd/commit/8a8889abdf60486c674df754c786222a24b08a30))
* consolidate release workflow triggers ([183c0fd](https://github.com/sayanmohsin/thingd/commit/183c0fdac72362f17b0145f8f232bcce3ec4c98b))
* consolidate release workflow triggers ([80cafcd](https://github.com/sayanmohsin/thingd/commit/80cafcdd6a5438700370e49fcb64a1925872d678))
* correct doc inaccuracies across api-spec, faq, runtime-env, cli-reference, AGENTS, README ([1829adb](https://github.com/sayanmohsin/thingd/commit/1829adb2ad40c70e250aa20e4846855c8c253a56))
* correct favicon href to include base path ([d8134aa](https://github.com/sayanmohsin/thingd/commit/d8134aae9ef5726038f06594d1e03432f5d68745))
* CORS — set permissive default, skip auth for OPTIONS, layer ordering ([bebf6c0](https://github.com/sayanmohsin/thingd/commit/bebf6c05478488ea4f9d5090ea4911ab30c31580))
* cross-compile macOS x64 native artifact ([#94](https://github.com/sayanmohsin/thingd/issues/94)) ([91874ba](https://github.com/sayanmohsin/thingd/commit/91874ba921a2a4980830194419041023a9b67247))
* dashboard package.json path resolution + add test-node to pre-push ([35ee66c](https://github.com/sayanmohsin/thingd/commit/35ee66cf136373976f07a504db23cd90a7c29472))
* disable lefthook during release to prevent push failures ([23dbd06](https://github.com/sayanmohsin/thingd/commit/23dbd0636374bf7b1abc3850d96f77ed696c770a))
* eliminate all biome noExplicitAny warnings ([7b392ce](https://github.com/sayanmohsin/thingd/commit/7b392ce5fcf0044f7042b39f7c0b416503d104c1))
* **engine:** guard search query against orphaned FTS entries ([c9f7de1](https://github.com/sayanmohsin/thingd/commit/c9f7de1282ba8e42bd8c2b2aa4043776b425a40c)), closes [#40](https://github.com/sayanmohsin/thingd/issues/40)
* **engine:** panic on clock error in debug, add Hash derives to model types ([d7321fd](https://github.com/sayanmohsin/thingd/commit/d7321fd115d60c53d06e5012fc6c19eaf3e8a14b))
* ensure API key is created for the correct project during cloud login ([1973ad5](https://github.com/sayanmohsin/thingd/commit/1973ad511685cc7eb62658a325ab16439bae0fdd))
* env var leak, CI caching, test timeouts, broken MCP→REST test ([c7e009a](https://github.com/sayanmohsin/thingd/commit/c7e009aa53bb75de493dcceff9142ecb35887914))
* error sanitization with production mode ([6744d69](https://github.com/sayanmohsin/thingd/commit/6744d69db3bffe65e2fbe43d73aaf38d0318dd03))
* exempt healthz and metrics from auth middleware ([10ab7ce](https://github.com/sayanmohsin/thingd/commit/10ab7ce4eb835acb191214571ceabfb4d746b9b8))
* force clean Docker rebuild with migrate feature ([17d3a1c](https://github.com/sayanmohsin/thingd/commit/17d3a1cb9eeec9897c4d1af62791b7ed7810e258))
* format main.rs ([8350ab0](https://github.com/sayanmohsin/thingd/commit/8350ab06d3477ae3d0b0134cc11555d320033a7c))
* index events in Tantivy, search returns both objects and events ([d47f798](https://github.com/sayanmohsin/thingd/commit/d47f798cbf62c59a0dce7dcafcfd51c4865a03de))
* inline particle component as h() function to prevent SSR hydration error ([98bcc60](https://github.com/sayanmohsin/thingd/commit/98bcc605ff9275b082dc52c0eac4ce5e73d5e235))
* inline particle component to prevent SSR hydration crash ([810a75d](https://github.com/sayanmohsin/thingd/commit/810a75d872c9740d9cfbfda9f310d278f808be72))
* InMemoryThingStore.search silently drops options.filter ([30b0fdd](https://github.com/sayanmohsin/thingd/commit/30b0fdd0e59f98e57554c0a83bff469c6f969f99))
* input validation hardening and CORS lockdown ([e530256](https://github.com/sayanmohsin/thingd/commit/e5302563773ede7aaf4d820f0be97bdeb6e7ba95))
* install cmake and disable cache for Docker build ([7611c32](https://github.com/sayanmohsin/thingd/commit/7611c32606a182a6c449203d672f22f4e3680bb9))
* isolate native prebuild artifacts ([#91](https://github.com/sayanmohsin/thingd/issues/91)) ([59ecf2f](https://github.com/sayanmohsin/thingd/commit/59ecf2f13c3bdc360889cd24f715252ddc1479aa))
* keep build-native dynamic name even when skipped ([5bbf0f2](https://github.com/sayanmohsin/thingd/commit/5bbf0f2c43fdf82ac412f911d85a11868e28a0d2))
* lint errors in thingd-cli — template literals, import type, import ordering ([9680888](https://github.com/sayanmohsin/thingd/commit/96808889f115659f5f2a5cca780ae8a84920a06a))
* logo character spacing, navbar sizing, and copyright year ([e494f05](https://github.com/sayanmohsin/thingd/commit/e494f05e23a9d0fffb2e7fc784faa5579be605f1))
* make crates release lookup idempotent ([768e405](https://github.com/sayanmohsin/thingd/commit/768e405c8e8728e4b784a62ed7521cc1b2ea59f0))
* make metadata generation cwd independent ([#74](https://github.com/sayanmohsin/thingd/issues/74)) ([d8eac3f](https://github.com/sayanmohsin/thingd/commit/d8eac3f2c946aaf60102cf723e4397e15f393c7b))
* make migrate feature default for thingd-server ([e1cc018](https://github.com/sayanmohsin/thingd/commit/e1cc01877f0913e5bf053d31cc2e0e7635bf7028))
* match logo style to thingd-cloud (sans-serif, explicit sizing) ([ba03e24](https://github.com/sayanmohsin/thingd/commit/ba03e2482f327dfee46bdebc4c13b0d727c5777f))
* mock cloud API in CLI tests to prevent real HTTP calls ([9748e53](https://github.com/sayanmohsin/thingd/commit/9748e530c3ed51007cf6f6608c5e737ecbca12ad))
* move prebuild merge after pnpm install in release workflow ([a45bae3](https://github.com/sayanmohsin/thingd/commit/a45bae31362b81ddd1a34519f127d9cd47410aed)), closes [#39](https://github.com/sayanmohsin/thingd/issues/39)
* NativeThingStore filter silently drops undefined values ([148d853](https://github.com/sayanmohsin/thingd/commit/148d853548b9984ae8dc3d7533e1d0a7b0a475df))
* navbar logo size and aspect ratio ([e22508b](https://github.com/sayanmohsin/thingd/commit/e22508b1950f070b4efed85ef84a13e1cea8acd3))
* navbar logo width to prevent right-side clipping ([2ef3059](https://github.com/sayanmohsin/thingd/commit/2ef305955ba61ac0d5d3e0d299e80d71d75f3c89))
* null-safe countActiveJobs in HttpThingStore and isolate count error handling in TUI ([3c239b7](https://github.com/sayanmohsin/thingd/commit/3c239b708507604fff738e2f1d0bcbc9ee8efbfe))
* only build amd64 Docker image (prebuilt binary is amd64) ([290376a](https://github.com/sayanmohsin/thingd/commit/290376aa2ecc564f2efcd7313dc2b66f3fe7a9ad))
* pass instance slug in REST requests for multi-instance isolation ([4942a83](https://github.com/sayanmohsin/thingd/commit/4942a83cad476ce305f312f24390652839735c93))
* prepare protected semantic release PRs ([f2a2d3b](https://github.com/sayanmohsin/thingd/commit/f2a2d3be158856b9404c8eb219aaa321c81b2447))
* prepare protected semantic release PRs ([5982c3c](https://github.com/sayanmohsin/thingd/commit/5982c3c2d764007a356aaff4a176b74c47b36e04))
* preserve instanceSlug on dashboard /api/connect reconnection ([45431a4](https://github.com/sayanmohsin/thingd/commit/45431a459c1956e672ebeb74ce374dffda163b39))
* preserve postgres connector ssl mode ([5396716](https://github.com/sayanmohsin/thingd/commit/5396716f10c7627f0e5336d8829c9517ed0ef7ac))
* prevent docs logo from clipping in navbar ([06b18cd](https://github.com/sayanmohsin/thingd/commit/06b18cd1fdffb9a350efc261659b3279e79554ce))
* prevent TUI crash when poll timer hits unhandled rejection ([2928c9b](https://github.com/sayanmohsin/thingd/commit/2928c9beaf4235c45f0a5005cb862cd15005ad0f))
* product description inconsistencies in docs ([900eaa3](https://github.com/sayanmohsin/thingd/commit/900eaa34bfdb2929427210f1919f5673fb2d4fbd))
* production readiness — security hardening and performance fixes ([4e401ca](https://github.com/sayanmohsin/thingd/commit/4e401caab90ac5ef6b5db91156ed10c76a90f9f4))
* propagate migrate feature from thingd dep ([fae4900](https://github.com/sayanmohsin/thingd/commit/fae490056aef13bd151bcb391e67362a689bc6c5))
* protect audit stream from deletion and injection ([44bd1d4](https://github.com/sayanmohsin/thingd/commit/44bd1d4793c8245b747f1670d3d4bcfd10059e70))
* quote native cleanup workflow command ([#92](https://github.com/sayanmohsin/thingd/issues/92)) ([a0727c5](https://github.com/sayanmohsin/thingd/commit/a0727c58257876b04b70b7d9da0d51527c9f5aef))
* read release version safely in docker job ([a7fefeb](https://github.com/sayanmohsin/thingd/commit/a7fefeb1e1e8164c759b3a4d00b62013eb5ed0a1))
* read release version safely in docker job ([f627257](https://github.com/sayanmohsin/thingd/commit/f6272573f0dab62f9b17ad30745e833178cc370b))
* rebuild native prebuild with correct filter (cache key + heredoc test) ([b474a59](https://github.com/sayanmohsin/thingd/commit/b474a59f15b0c1269073708dc26957d73ff5bf83))
* reduce TUI screen refresh with parallelized network, dirty flag, and concurrency guard ([f1d034b](https://github.com/sayanmohsin/thingd/commit/f1d034b560a1367803988a0cda9f86da20b7d211))
* refactor migration into function, trigger on first authenticated request ([48a2723](https://github.com/sayanmohsin/thingd/commit/48a2723a68b98fca029e3dc6b0699b0e85321194))
* release check-release uses --format=%s not --oneline ([19a587f](https://github.com/sayanmohsin/thingd/commit/19a587f01d6796ace7f0f47448bc1b32b0ec1a92))
* **release:** sync path dep version pins on version bump ([96b1e24](https://github.com/sayanmohsin/thingd/commit/96b1e2441cb5303cb663c6fa1d77bb754f8e8856))
* remaining audit items — query length, CSRF, fallback warning, FTS5 optimize ([2f48ac6](https://github.com/sayanmohsin/thingd/commit/2f48ac6910352058c11a75603a778038432c376d))
* remove duplicate hero logo, redesign SVG with gradients and better glow ([270b09f](https://github.com/sayanmohsin/thingd/commit/270b09ff0aa2e5cdbb51afa8d965f488dc83ef9e))
* remove extra spacing around docs logo braces ([2a4b157](https://github.com/sayanmohsin/thingd/commit/2a4b157350cda50e45a1c12817151c9ea94c5a07))
* remove hardcoded pnpm version from deploy-docs workflow (use packageManager from package.json) ([ad6ce6d](https://github.com/sayanmohsin/thingd/commit/ad6ce6de3b926142882d36790f3b7a030411d9ef))
* remove set -e from tenant isolation test script ([efcbdbd](https://github.com/sayanmohsin/thingd/commit/efcbdbdb1a12b981dda48c31b7ce98501066c5cb))
* remove stale comment in CI check-release regex [skip ci] ([0af2864](https://github.com/sayanmohsin/thingd/commit/0af2864dbc200bec8489be8c3aa04aaa89e11d6f))
* remove test-rust and test-cli from pre-push hook (CI only) ([ae5fe78](https://github.com/sayanmohsin/thingd/commit/ae5fe78aacafcfec727334d4270f7091e801a75a))
* remove whitespace between docs logo tspans ([4e83d39](https://github.com/sayanmohsin/thingd/commit/4e83d39a24679097dbde75bf0b268ab68f7c6cd4))
* remove wrong /api prefix from cloud CLI endpoints ([38f0c21](https://github.com/sayanmohsin/thingd/commit/38f0c2149b074db40f905d6a070480a1ca6eb8f4))
* rename QUICKSTART.md to quickstart.md (case-sensitive GitHub Pages), hide nav text, enlarge logo ([817ed0b](https://github.com/sayanmohsin/thingd/commit/817ed0bebf7af5c4637ddd6c44084781cd53f82a))
* rename QUICKSTART.md to quickstart.md for case-sensitive FS ([3c5102f](https://github.com/sayanmohsin/thingd/commit/3c5102fa3ec469b767039abe7902a6d2045cc2e1))
* rename QUICKSTART.md to quickstart.md, enlarge navbar logo ([a69f0f5](https://github.com/sayanmohsin/thingd/commit/a69f0f5218d08a7bc209274e6c1193377fe5463c))
* replace node -e with heredoc in CI workflows ([7c5d347](https://github.com/sayanmohsin/thingd/commit/7c5d3470cae976637fd43ddbe9fbcf20045ab654))
* replace release PR autobump with semantic-release ([#90](https://github.com/sayanmohsin/thingd/issues/90)) ([949945a](https://github.com/sayanmohsin/thingd/commit/949945a7ab5ac93c13b5675454f08f66f588d8aa))
* replication runner race condition with AbortController ([141e528](https://github.com/sayanmohsin/thingd/commit/141e528d04b11e57326b720d72895d9bb50ac3a7))
* resolve --driver cloud flag to load cloud config and use REST URL ([46631b4](https://github.com/sayanmohsin/thingd/commit/46631b48934dcdb5ebb463c1e6ff584ce3cc232a))
* resolve all 24 audit issues across 4 phases ([1de3903](https://github.com/sayanmohsin/thingd/commit/1de3903efc521a1d65dcd8a5b5261e18aaef3342))
* restore original logo sizing, update footer style ([857571b](https://github.com/sayanmohsin/thingd/commit/857571be80df5a3b769376314ca50067d415f1dc))
* return empty results for empty search query instead of 400 ([5cc6163](https://github.com/sayanmohsin/thingd/commit/5cc6163fc3ea965a6a373482201baa26119c1e4f))
* **sdk:** concurrent batch ops, timeout, body size limit, error propagation ([1d802fe](https://github.com/sayanmohsin/thingd/commit/1d802fe5a3203d1c9f9c452a2dcf060efdd61954))
* **sdk:** use HttpThingStore (REST) for cloud driver instead of MCP ([2f7abc5](https://github.com/sayanmohsin/thingd/commit/2f7abc5d1dd0633276e24e02fb452fbefe639687))
* security and performance audit fixes ([fc81b34](https://github.com/sayanmohsin/thingd/commit/fc81b3450996669a274c4052d8b87fb822bbbb8b))
* **server:** harden security boundaries ([0961ffe](https://github.com/sayanmohsin/thingd/commit/0961ffed1b6511c52b3c8e06dca71db568b01692))
* **server:** harden tenant and connector security ([e232d65](https://github.com/sayanmohsin/thingd/commit/e232d655519a44ac5664c8957d0bec233fc17a2b))
* **server:** repair broken build — add tenant_config to test AppState, fix formatting, remove unused import ([1fbefee](https://github.com/sayanmohsin/thingd/commit/1fbefee034400bde55017ffe72fbcf7504d6ee44)), closes [#60](https://github.com/sayanmohsin/thingd/issues/60)
* show cloud login hint on driver selection screen when no config ([dca7cd4](https://github.com/sayanmohsin/thingd/commit/dca7cd498f59a4881aab60961672e019358d5e3a))
* **sidecar:** add graceful shutdown handler for SIGINT and SIGTERM ([1093989](https://github.com/sayanmohsin/thingd/commit/109398938db526a347d6e59223e98d3b32ee9a20))
* **sidecar:** align MCP tools with SDK — audit events, annotations, validation, optional stream ([a7f5007](https://github.com/sayanmohsin/thingd/commit/a7f5007391dd4e0c9ece19f280446f0859efe2e0))
* **sidecar:** align queue push, audit source, and search allowlist with SDK ([d951595](https://github.com/sayanmohsin/thingd/commit/d951595f89ff1dc3c7651ea6891844c35e8dee02))
* **sidecar:** constant-time auth, AppState token, production-mode MCP sanitization ([9a92d81](https://github.com/sayanmohsin/thingd/commit/9a92d81914edc17de0f9f652ec4b46c86746f6a6))
* **sidecar:** include object body in GET /v1/objects response ([e8557ed](https://github.com/sayanmohsin/thingd/commit/e8557ed9b7f50efb68680cf6c870ac134abcb807))
* **sidecar:** loud fallback on SQLite failure + track has_fallback status ([d16419e](https://github.com/sayanmohsin/thingd/commit/d16419e366e965c537d99fafe776a697a36ef285))
* **sidecar:** rate limiter hardening and cluster status from real config ([1f9ba9c](https://github.com/sayanmohsin/thingd/commit/1f9ba9c9648d50e46cd415193ab6926e849fc0e9))
* **sidecar:** wire request_timeout_secs into tower timeout middleware ([96c6a9a](https://github.com/sayanmohsin/thingd/commit/96c6a9a84e6ce9be6c4cb4882a4d4372601b3e43))
* skip broken CLI tests, fix rmSync for Fjall directories ([29d7a4f](https://github.com/sayanmohsin/thingd/commit/29d7a4feb1161291b9d4d9dd51df34c2fcbaf481))
* skip native reopen test (Fjall single-process), fix clippy ([423e71a](https://github.com/sayanmohsin/thingd/commit/423e71a361e0109744137e645454c318df4db5a0))
* skip pnpm test:package on Windows (spawn cmd.exe not available) ([83faf17](https://github.com/sayanmohsin/thingd/commit/83faf17a459c7bd4e931748540e8ab79988ab0e1))
* skip pre-push hooks during semantic-release git push ([54d1b05](https://github.com/sayanmohsin/thingd/commit/54d1b055956856787120afc80e46a8026eff43ad))
* slow TUI poll interval from 2s to 10s ([8937108](https://github.com/sayanmohsin/thingd/commit/8937108c1316a6f05101e8c733316b940d24fb96))
* speed up cluster tests with adaptive polling, deduplicate logs ([f739edc](https://github.com/sayanmohsin/thingd/commit/f739edc8258a7aafb1b98671e90b35225770c12b))
* statically link thingd-server binary for scratch-based Docker image ([c95c5e6](https://github.com/sayanmohsin/thingd/commit/c95c5e637f4c2185a0912f49c20c739a1db2d018))
* support workspace cargo dependency versions ([#88](https://github.com/sayanmohsin/thingd/issues/88)) ([4e097ad](https://github.com/sayanmohsin/thingd/commit/4e097ad733b8576708899f98bc018ea79e3660c0))
* sync Cargo.lock after workspace version bump ([b83eca7](https://github.com/sayanmohsin/thingd/commit/b83eca76f783a0f08e39e4dd3a64a7925190b1e9))
* sync version pins after 0.41.0 release bump ([6fb48e1](https://github.com/sayanmohsin/thingd/commit/6fb48e163d6672f4585e6f6cca1d5a96f9412aed))
* test native filter on every build-native matrix runner ([7f0b178](https://github.com/sayanmohsin/thingd/commit/7f0b1784aa6cb56e02f8e3ebde73bc883c10df2d))
* tolerate cargo dependency declaration formats ([#89](https://github.com/sayanmohsin/thingd/issues/89)) ([b1f3d98](https://github.com/sayanmohsin/thingd/commit/b1f3d981e5d4c3e8ada6e7104338e3fe27134636))
* tolerate missing release doc version examples ([3a7cd78](https://github.com/sayanmohsin/thingd/commit/3a7cd78fef93d59dbe6242a5d7845dbf2ff9033c))
* tolerate missing release doc version examples ([d8df795](https://github.com/sayanmohsin/thingd/commit/d8df7957f7cbe81787bcfc89a576c810e4bd7fca))
* trigger crates.io publish for v0.58.1 ([017b9a0](https://github.com/sayanmohsin/thingd/commit/017b9a02b7de10c6e19df33cd461d234eeb21447))
* TUI cloud selection falls through to manual form when instance discovery fails ([8666634](https://github.com/sayanmohsin/thingd/commit/8666634150dd2ce9c92fa0bf441eeb299e57c2e1))
* **tui:** derive REST base URL from mcpUrl for cloud connection ([ab6f651](https://github.com/sayanmohsin/thingd/commit/ab6f6512703f3736798c7369a9c196f1c0bc035d))
* **tui:** populate objectsByCollection so collection tree shows real data ([6372e17](https://github.com/sayanmohsin/thingd/commit/6372e1740c782aa460781f25098e018a50b09cea))
* **tui:** preserve search results from poll timer overwrite ([c17ce7d](https://github.com/sayanmohsin/thingd/commit/c17ce7df7c5f4ab08b0b6cac082930b461c7ee91))
* **tui:** rename 'Integrity Check' to 'Health Check', show counts ([ee5541c](https://github.com/sayanmohsin/thingd/commit/ee5541c5f77725a365ac972662573caae6b72f40))
* **tui:** replace hardcoded load-test artifact names with empty defaults ([ffcd3a9](https://github.com/sayanmohsin/thingd/commit/ffcd3a9de9bd7a612a6a8f0f7f84680b7c483dbb))
* **tui:** surface cloud REST API errors in metrics and info screens ([cbd46df](https://github.com/sayanmohsin/thingd/commit/cbd46df501d94208df4599fda10c0565218b64be))
* **tui:** surface delete() result feedback ([852e220](https://github.com/sayanmohsin/thingd/commit/852e220c09cdcb7df9033a284529b1faff89622e))
* **tui:** use defaultThingdDbPath instead of ~/Downloads/data.db ([5d670e3](https://github.com/sayanmohsin/thingd/commit/5d670e3ad2ef2190acc18e25bcc9f022fbe9f28d))
* **tui:** validate cloud slugs before constructing URL ([3eae637](https://github.com/sayanmohsin/thingd/commit/3eae637d06eaa48792ec28e7341f146df007722d))
* update AGENTS.md Rust test count (187 → 219) ([b497819](https://github.com/sayanmohsin/thingd/commit/b497819bc3c9f8a9e4887840dfde6f0a4503fbc4))
* update AGENTS.md test counts to match actual (61/44/187) ([7497166](https://github.com/sayanmohsin/thingd/commit/7497166de258935e92a1c231373a2f3be662f4f4))
* update Cargo dependency version from 0.38 to 0.39 ([b037656](https://github.com/sayanmohsin/thingd/commit/b03765603ceb2d07fe82be6e91ddc193374c1f4a))
* use allowBuilds in pnpm-workspace.yaml for docs deploy (pnpm 11 migration) ([d6ce40d](https://github.com/sayanmohsin/thingd/commit/d6ce40d01be4247ec63b85a883b11d6a155a541c))
* use assert_ne! instead of assert!(!=) — clippy ([e814397](https://github.com/sayanmohsin/thingd/commit/e8143976d67f1893662511307b5dcd2044fc3be3))
* use bash for matrix native builds ([#95](https://github.com/sayanmohsin/thingd/issues/95)) ([3bea084](https://github.com/sayanmohsin/thingd/commit/3bea084fdfbaa5b9966ffbb57c43055a73bcfbc2))
* use cargo rustc for static linking to avoid proc-macro conflict ([1b71bcf](https://github.com/sayanmohsin/thingd/commit/1b71bcf00d1ae78043fb06f439e75356763267d0))
* use cargo rustc for static linking to avoid proc-macro conflict ([31f7ec7](https://github.com/sayanmohsin/thingd/commit/31f7ec7b61087b5ed4c7665e6bf8987aa80a0e5c))
* use env vars instead of CLI args in tenant isolation test ([4d8c3b4](https://github.com/sayanmohsin/thingd/commit/4d8c3b4e04cb1e1854eb0cf779372598dfec1027))
* use fixed ports with matching advertiseUrl in cluster status test ([96727b2](https://github.com/sayanmohsin/thingd/commit/96727b2f038cfcbb2de58839e3101df2c1a6f577))
* use fixed ports with matching advertiseUrl in cluster status test ([4c51221](https://github.com/sayanmohsin/thingd/commit/4c5122163e58d7957aae19d615220535a2a41334))
* use monospace for even docs logo character spacing ([dc1326d](https://github.com/sayanmohsin/thingd/commit/dc1326d0baffb910e8b822fe04787cdcf9048922))
* use pnpm install without --frozen-lockfile for docs deploy ([bce37ab](https://github.com/sayanmohsin/thingd/commit/bce37abcdf682bacda4a2c4ccc9361d9ad33e17f))
* use release PRs for protected main releases ([#96](https://github.com/sayanmohsin/thingd/issues/96)) ([d61b020](https://github.com/sayanmohsin/thingd/commit/d61b0207ba3222948ece77364ff7cf6ba9dfa729))
* use REST protocol for cloud driver, add @thingd/client package, fix docs ([c53b21f](https://github.com/sayanmohsin/thingd/commit/c53b21f22fecfb44a677c8f1c0f8e35671daabcf))
* use separate &lt;text&gt; elements to avoid } overlap in logo ([e0a53d1](https://github.com/sayanmohsin/thingd/commit/e0a53d18ab8d8a54169e26bc1e5cee5fd9cbb3c7))
* use shell bash for native filter test on Windows runners ([10e916c](https://github.com/sayanmohsin/thingd/commit/10e916cf10f3d02551310b951b65a52df89fc282))
* use static job name for build-native to avoid raw template in skipped matrix jobs ([99c0ec3](https://github.com/sayanmohsin/thingd/commit/99c0ec3788611912f9f8ded36d688fe71a18e31e))
* use tspan for logo text (no hardcoded x positions) ([6414fd4](https://github.com/sayanmohsin/thingd/commit/6414fd4be7c0c6a6cc54852e2e1922195e66d5e2))
* use unique temp path for native filter test to avoid CI conflicts ([0a168b0](https://github.com/sayanmohsin/thingd/commit/0a168b0c94982b0ed7757f8f5f2c3a49bbf0a672))
* wrap CI inline test in async IIFE to avoid ESM/require conflict ([001f0e7](https://github.com/sayanmohsin/thingd/commit/001f0e7c0abb9610070895b3701c06d987d571e0))


### Performance Improvements

* add benchmarks, sidecar tests, and optimize mutex type ([7e3a690](https://github.com/sayanmohsin/thingd/commit/7e3a690f69eb545ebb892888fc1d68ac3c73a2cc))
* **engine:** batch multi-row INSERT for put_objects_batch and append_events_batch ([c4cfd13](https://github.com/sayanmohsin/thingd/commit/c4cfd134e18a6649dce5fe6249fa6ab6f829f1a0))
* **engine:** push FTS collection filter and LIMIT to SQL, optimize delete_last_event ([8ef0e75](https://github.com/sayanmohsin/thingd/commit/8ef0e7550a21834609183c6c9649156a7f0626da))
* optimize SQLite upsert and batch delete ([10867c6](https://github.com/sayanmohsin/thingd/commit/10867c60873765234d306b19dd5cdaadab90ff3d))
* **sidecar:** reader/writer connection pool for concurrent reads ([1f2ea69](https://github.com/sayanmohsin/thingd/commit/1f2ea6914ba40bfa4f08bcfea0e1ef2176671afe))

## [0.71.0](https://github.com/sayanmohsin/thingd/compare/v0.70.0...v0.71.0) (2026-07-31)

## [0.70.0](https://github.com/sayanmohsin/thingd/compare/v0.69.1...v0.70.0) (2026-07-29)

## [0.69.1](https://github.com/sayanmohsin/thingd/compare/v0.69.0...v0.69.1) (2026-07-28)

## [0.69.0](https://github.com/sayanmohsin/thingd/compare/v0.68.6...v0.69.0) (2026-07-28)

## [0.68.6](https://github.com/sayanmohsin/thingd/compare/v0.68.5...v0.68.6) (2026-07-26)

## [0.68.5](https://github.com/sayanmohsin/thingd/compare/v0.68.4...v0.68.5) (2026-07-24)

## [0.68.4](https://github.com/sayanmohsin/thingd/compare/v0.68.3...v0.68.4) (2026-07-24)

## [0.68.3](https://github.com/sayanmohsin/thingd/compare/v0.68.2...v0.68.3) (2026-07-24)

## [0.68.2](https://github.com/sayanmohsin/thingd/compare/v0.68.1...v0.68.2) (2026-07-24)

## [0.68.1](https://github.com/sayanmohsin/thingd/compare/v0.68.0...v0.68.1) (2026-07-24)

## [0.68.0](https://github.com/sayanmohsin/thingd/compare/v0.67.3...v0.68.0) (2026-07-24)

## [0.67.3](https://github.com/sayanmohsin/thingd/compare/v0.67.2...v0.67.3) (2026-07-24)

## [0.67.2](https://github.com/sayanmohsin/thingd/compare/v0.67.1...v0.67.2) (2026-07-24)

## [0.67.1](https://github.com/sayanmohsin/thingd/compare/v0.67.0...v0.67.1) (2026-07-24)

## [0.67.0](https://github.com/sayanmohsin/thingd/compare/v0.66.0...v0.67.0) (2026-07-24)

## [0.66.0](https://github.com/sayanmohsin/thingd/compare/v0.65.9...v0.66.0) (2026-07-23)

## [0.65.9](https://github.com/sayanmohsin/thingd/compare/v0.65.8...v0.65.9) (2026-07-19)

## [0.65.8](https://github.com/sayanmohsin/thingd/compare/v0.65.7...v0.65.8) (2026-07-19)

## [0.65.7](https://github.com/sayanmohsin/thingd/compare/v0.65.6...v0.65.7) (2026-07-19)

## [0.65.6](https://github.com/sayanmohsin/thingd/compare/v0.65.5...v0.65.6) (2026-07-19)

## [0.65.5](https://github.com/sayanmohsin/thingd/compare/v0.65.4...v0.65.5) (2026-07-19)

## [0.65.4](https://github.com/sayanmohsin/thingd/compare/v0.65.3...v0.65.4) (2026-07-19)

## [0.65.3](https://github.com/sayanmohsin/thingd/compare/v0.65.2...v0.65.3) (2026-07-18)

## [0.65.2](https://github.com/sayanmohsin/thingd/compare/v0.65.1...v0.65.2) (2026-07-18)

## [0.65.1](https://github.com/sayanmohsin/thingd/compare/v0.65.0...v0.65.1) (2026-07-18)

## [0.65.0](https://github.com/sayanmohsin/thingd/compare/v0.64.1...v0.65.0) (2026-07-18)

## [0.64.1](https://github.com/sayanmohsin/thingd/compare/v0.64.0...v0.64.1) (2026-07-18)

## [0.64.0](https://github.com/sayanmohsin/thingd/compare/v0.63.0...v0.64.0) (2026-07-18)

## [0.63.0](https://github.com/sayanmohsin/thingd/compare/v0.62.0...v0.63.0) (2026-07-18)

## [0.62.0](https://github.com/sayanmohsin/thingd/compare/v0.61.0...v0.62.0) (2026-07-17)

## [0.61.0](https://github.com/sayanmohsin/thingd/compare/v0.60.0...v0.61.0) (2026-07-15)

## [0.60.0](https://github.com/sayanmohsin/thingd/compare/v0.59.0...v0.60.0) (2026-07-15)

## [0.59.0](https://github.com/sayanmohsin/thingd/compare/v0.58.2...v0.59.0) (2026-07-15)

## [0.58.2](https://github.com/sayanmohsin/thingd/compare/v0.58.1...v0.58.2) (2026-07-15)

## [0.58.1](https://github.com/sayanmohsin/thingd/compare/v0.58.0...v0.58.1) (2026-07-15)

## [0.58.0](https://github.com/sayanmohsin/thingd/compare/v0.57.0...v0.58.0) (2026-07-15)

## [0.57.0](https://github.com/sayanmohsin/thingd/compare/v0.56.0...v0.57.0) (2026-07-15)

## [0.56.0](https://github.com/sayanmohsin/thingd/compare/v0.55.0...v0.56.0) (2026-07-15)

## [0.55.0](https://github.com/sayanmohsin/thingd/compare/v0.54.0...v0.55.0) (2026-07-15)

## [0.54.0](https://github.com/sayanmohsin/thingd/compare/v0.53.2...v0.54.0) (2026-07-15)

## [0.53.2](https://github.com/sayanmohsin/thingd/compare/v0.53.1...v0.53.2) (2026-07-15)

## [0.53.1](https://github.com/sayanmohsin/thingd/compare/v0.53.0...v0.53.1) (2026-07-15)

## [0.53.0](https://github.com/sayanmohsin/thingd/compare/v0.52.10...v0.53.0) (2026-07-15)

## [0.52.10](https://github.com/sayanmohsin/thingd/compare/v0.52.9...v0.52.10) (2026-07-15)

## [0.52.9](https://github.com/sayanmohsin/thingd/compare/v0.52.8...v0.52.9) (2026-07-15)

## [0.52.8](https://github.com/sayanmohsin/thingd/compare/v0.52.7...v0.52.8) (2026-07-15)

## [0.52.7](https://github.com/sayanmohsin/thingd/compare/v0.52.6...v0.52.7) (2026-07-15)

## [0.52.6](https://github.com/sayanmohsin/thingd/compare/v0.52.5...v0.52.6) (2026-07-15)

## [0.52.5](https://github.com/sayanmohsin/thingd/compare/v0.52.4...v0.52.5) (2026-07-15)

## [0.52.4](https://github.com/sayanmohsin/thingd/compare/v0.52.3...v0.52.4) (2026-07-15)

## [0.52.3](https://github.com/sayanmohsin/thingd/compare/v0.52.2...v0.52.3) (2026-07-15)

## [0.52.2](https://github.com/sayanmohsin/thingd/compare/v0.52.1...v0.52.2) (2026-07-15)

## [0.52.1](https://github.com/sayanmohsin/thingd/compare/v0.52.0...v0.52.1) (2026-07-14)

## [0.52.0](https://github.com/sayanmohsin/thingd/compare/v0.51.4...v0.52.0) (2026-07-14)

## [0.51.4](https://github.com/sayanmohsin/thingd/compare/v0.51.3...v0.51.4) (2026-07-14)

## [0.51.3](https://github.com/sayanmohsin/thingd/compare/v0.51.2...v0.51.3) (2026-07-14)

## [0.51.2](https://github.com/sayanmohsin/thingd/compare/v0.51.1...v0.51.2) (2026-07-14)

## [0.51.1](https://github.com/sayanmohsin/thingd/compare/v0.51.0...v0.51.1) (2026-07-14)

## [0.51.0](https://github.com/sayanmohsin/thingd/compare/v0.50.5...v0.51.0) (2026-07-14)

## [0.50.5](https://github.com/sayanmohsin/thingd/compare/v0.50.4...v0.50.5) (2026-07-14)

## [0.50.4](https://github.com/sayanmohsin/thingd/compare/v0.50.3...v0.50.4) (2026-07-14)

## [0.50.3](https://github.com/sayanmohsin/thingd/compare/v0.50.2...v0.50.3) (2026-07-14)

## [0.50.2](https://github.com/sayanmohsin/thingd/compare/v0.50.1...v0.50.2) (2026-07-14)

## [0.50.1](https://github.com/sayanmohsin/thingd/compare/v0.50.0...v0.50.1) (2026-07-14)

## [0.50.0](https://github.com/sayanmohsin/thingd/compare/v0.49.9...v0.50.0) (2026-07-14)

## [0.49.9](https://github.com/sayanmohsin/thingd/compare/v0.49.8...v0.49.9) (2026-07-13)

## [0.49.8](https://github.com/sayanmohsin/thingd/compare/v0.49.7...v0.49.8) (2026-07-13)

## [0.49.7](https://github.com/sayanmohsin/thingd/compare/v0.49.6...v0.49.7) (2026-07-13)

## [0.49.6](https://github.com/sayanmohsin/thingd/compare/v0.49.5...v0.49.6) (2026-07-13)

## [0.49.5](https://github.com/sayanmohsin/thingd/compare/v0.49.4...v0.49.5) (2026-07-13)

## [0.49.4](https://github.com/sayanmohsin/thingd/compare/v0.49.3...v0.49.4) (2026-07-13)

## [0.49.3](https://github.com/sayanmohsin/thingd/compare/v0.49.2...v0.49.3) (2026-07-12)

## [0.49.2](https://github.com/sayanmohsin/thingd/compare/v0.49.1...v0.49.2) (2026-07-12)

## [0.49.1](https://github.com/sayanmohsin/thingd/compare/v0.49.0...v0.49.1) (2026-07-10)

## [0.49.0](https://github.com/sayanmohsin/thingd/compare/v0.48.1...v0.49.0) (2026-07-09)

## [0.48.1](https://github.com/sayanmohsin/thingd/compare/v0.48.0...v0.48.1) (2026-07-08)

### Bug Fixes

* add require condition to exports for CJS compatibility ([8ad7357](https://github.com/sayanmohsin/thingd/commit/8ad7357892ac16abd446ce58de14478baf24664d))

## [0.48.0](https://github.com/sayanmohsin/thingd/compare/v0.47.5...v0.48.0) (2026-07-05)

### Features

* **connectors:** add ping endpoint and test-connection UI ([236020f](https://github.com/sayanmohsin/thingd/commit/236020f1d5577b8a3cc745a2ae598852efeae516))
* **connectors:** wire Postgres/MySQL connectors through all layers ([4cc1076](https://github.com/sayanmohsin/thingd/commit/4cc1076be3b9e2e05f146d894f384cb8ef0cb4a8))

## [0.47.5](https://github.com/sayanmohsin/thingd/compare/v0.47.4...v0.47.5) (2026-07-04)

### Bug Fixes

* **ci:** resolve cargo-deny and vitepress build failures ([c622de4](https://github.com/sayanmohsin/thingd/commit/c622de46162298c0081cd5ca9fc5e8421458d960))

## [0.47.4](https://github.com/sayanmohsin/thingd/compare/v0.47.3...v0.47.4) (2026-07-03)

### Bug Fixes

* protect audit stream from deletion and injection ([44bd1d4](https://github.com/sayanmohsin/thingd/commit/44bd1d4793c8245b747f1670d3d4bcfd10059e70))

## [0.47.3](https://github.com/sayanmohsin/thingd/compare/v0.47.2...v0.47.3) (2026-07-03)

### Bug Fixes

* **cli:** update 14 stale cloud API paths after thingd-cloud route refactor ([60ecf7e](https://github.com/sayanmohsin/thingd/commit/60ecf7e1ee9e7e5d90c61879caaf092316d92ed3))

## [0.47.2](https://github.com/sayanmohsin/thingd/compare/v0.47.1...v0.47.2) (2026-07-03)

### Bug Fixes

* **cli:** cloud connect form now uses project/instance slug picker instead of raw URL ([e9f2ba9](https://github.com/sayanmohsin/thingd/commit/e9f2ba9dc0610e487746197089ab3275b644dd60))

## [0.47.1](https://github.com/sayanmohsin/thingd/compare/v0.47.0...v0.47.1) (2026-07-03)

### Bug Fixes

* **cli:** cloud TUI connect now uses instance MCP URL instead of API base URL ([a9ce86f](https://github.com/sayanmohsin/thingd/commit/a9ce86f82fc40fdaf87ea11d405a4e914b2e2dd9))

## [0.47.0](https://github.com/sayanmohsin/thingd/compare/v0.46.0...v0.47.0) (2026-07-03)

### Features

* **connectors:** add list_tables() to Connector trait ([38a7287](https://github.com/sayanmohsin/thingd/commit/38a7287e5d993e3cf2cbd26a7fb6570808edc858))

## [0.46.0](https://github.com/sayanmohsin/thingd/compare/v0.45.1...v0.46.0) (2026-07-02)

### Features

* **connectors:** Postgres and MySQL connectors with streaming PullStream ([8cdc45d](https://github.com/sayanmohsin/thingd/commit/8cdc45dd274a60f0099f5cc76377c3ad027794fa))

## [0.45.1](https://github.com/sayanmohsin/thingd/compare/v0.45.0...v0.45.1) (2026-07-01)

### Performance Improvements

* **sidecar:** reader/writer connection pool for concurrent reads ([1f2ea69](https://github.com/sayanmohsin/thingd/commit/1f2ea6914ba40bfa4f08bcfea0e1ef2176671afe))

## [0.45.0](https://github.com/sayanmohsin/thingd/compare/v0.44.3...v0.45.0) (2026-06-30)

### Features

* centralize MCP tool count in constants.ts, add to VitePress theme config, update docs ([20eb79b](https://github.com/sayanmohsin/thingd/commit/20eb79b2adcdfde730b69fd33e2d848b977a7cfa))

### Bug Fixes

* product description inconsistencies in docs ([900eaa3](https://github.com/sayanmohsin/thingd/commit/900eaa34bfdb2929427210f1919f5673fb2d4fbd))

### Performance Improvements

* **engine:** batch multi-row INSERT for put_objects_batch and append_events_batch ([c4cfd13](https://github.com/sayanmohsin/thingd/commit/c4cfd134e18a6649dce5fe6249fa6ab6f829f1a0))

## [0.44.3](https://github.com/sayanmohsin/thingd/compare/v0.44.2...v0.44.3) (2026-06-30)

### Bug Fixes

* remaining audit items — query length, CSRF, fallback warning, FTS5 optimize ([2f48ac6](https://github.com/sayanmohsin/thingd/commit/2f48ac6910352058c11a75603a778038432c376d))

## [0.44.2](https://github.com/sayanmohsin/thingd/compare/v0.44.1...v0.44.2) (2026-06-30)

### Bug Fixes

* production readiness — security hardening and performance fixes ([4e401ca](https://github.com/sayanmohsin/thingd/commit/4e401caab90ac5ef6b5db91156ed10c76a90f9f4))

## [0.44.1](https://github.com/sayanmohsin/thingd/compare/v0.44.0...v0.44.1) (2026-06-30)

### Bug Fixes

* security and performance audit fixes ([fc81b34](https://github.com/sayanmohsin/thingd/commit/fc81b3450996669a274c4052d8b87fb822bbbb8b))

## [0.44.0](https://github.com/sayanmohsin/thingd/compare/v0.43.0...v0.44.0) (2026-06-30)

### Features

* **engine:** add event idempotency via idempotencyKey ([cf77768](https://github.com/sayanmohsin/thingd/commit/cf77768e9ccd0da022918daf6a1499e459ed9eb3)), closes [#49](https://github.com/sayanmohsin/thingd/issues/49)
* **sidecar:** add /metrics endpoint with Prometheus-formatted store metrics ([9a5f15b](https://github.com/sayanmohsin/thingd/commit/9a5f15bea80c72054e1192309d583c3ef673a714)), closes [#46](https://github.com/sayanmohsin/thingd/issues/46)

## [0.43.0](https://github.com/sayanmohsin/thingd/compare/v0.42.1...v0.43.0) (2026-06-30)

### Features

* **engine:** add optimistic locking / CAS support to put() ([2cc69f1](https://github.com/sayanmohsin/thingd/commit/2cc69f19bb0e54ca81d6efe23a264797cf81d14c)), closes [#43](https://github.com/sayanmohsin/thingd/issues/43)

## [0.42.1](https://github.com/sayanmohsin/thingd/compare/v0.42.0...v0.42.1) (2026-06-30)

### Bug Fixes

* **engine:** guard search query against orphaned FTS entries ([c9f7de1](https://github.com/sayanmohsin/thingd/commit/c9f7de1282ba8e42bd8c2b2aa4043776b425a40c)), closes [#40](https://github.com/sayanmohsin/thingd/issues/40)

## [0.42.0](https://github.com/sayanmohsin/thingd/compare/v0.41.3...v0.42.0) (2026-06-30)

### Features

* **cli:** use cloud login credentials for TUI, dashboard, and CLI ([13b2e7f](https://github.com/sayanmohsin/thingd/commit/13b2e7fe1c510e8d4a19d013760a59d5bf7806e6))

## [0.41.3](https://github.com/sayanmohsin/thingd/compare/v0.41.2...v0.41.3) (2026-06-30)

### Bug Fixes

* **sidecar:** align queue push, audit source, and search allowlist with SDK ([d951595](https://github.com/sayanmohsin/thingd/commit/d951595f89ff1dc3c7651ea6891844c35e8dee02))

## [0.41.2](https://github.com/sayanmohsin/thingd/compare/v0.41.1...v0.41.2) (2026-06-30)

### Bug Fixes

* **sidecar:** align MCP tools with SDK — audit events, annotations, validation, optional stream ([a7f5007](https://github.com/sayanmohsin/thingd/commit/a7f5007391dd4e0c9ece19f280446f0859efe2e0))

## [0.41.1](https://github.com/sayanmohsin/thingd/compare/v0.41.0...v0.41.1) (2026-06-30)

### Bug Fixes

* auto-fix biome lint errors in cloud.ts ([afaae7a](https://github.com/sayanmohsin/thingd/commit/afaae7a44eadb35c3bd084d54b2bd51d0a565326))
* mock cloud API in CLI tests to prevent real HTTP calls ([9748e53](https://github.com/sayanmohsin/thingd/commit/9748e530c3ed51007cf6f6608c5e737ecbca12ad))
* **release:** sync path dep version pins on version bump ([96b1e24](https://github.com/sayanmohsin/thingd/commit/96b1e2441cb5303cb663c6fa1d77bb754f8e8856))
* sync version pins after 0.41.0 release bump ([6fb48e1](https://github.com/sayanmohsin/thingd/commit/6fb48e163d6672f4585e6f6cca1d5a96f9412aed))

## [0.41.0](https://github.com/sayanmohsin/thingd/compare/v0.40.3...v0.41.0) (2026-06-29)

### Features

* **cli:** add organization subcommands to cloud module ([e9fc5cb](https://github.com/sayanmohsin/thingd/commit/e9fc5cb048c4c6967aafa8e6e38ec8de7db72033))

## [0.40.3](https://github.com/sayanmohsin/thingd/compare/v0.40.2...v0.40.3) (2026-06-29)

### Bug Fixes

* add pnpm test:cli and pnpm test:rust to pre-push hook (was missing, causing CI failures) ([65a100b](https://github.com/sayanmohsin/thingd/commit/65a100b28b4bca0a352be1368e82cb175c7ae28d))

## [0.40.2](https://github.com/sayanmohsin/thingd/compare/v0.40.1...v0.40.2) (2026-06-29)

### Bug Fixes

* change VitePress base from /thingd/ to / for custom domain ([3308cc3](https://github.com/sayanmohsin/thingd/commit/3308cc3c41d586ac29740aa9a4280ec4ebda7756))

## [0.40.1](https://github.com/sayanmohsin/thingd/compare/v0.40.0...v0.40.1) (2026-06-29)

### Bug Fixes

* lint errors in thingd-cli — template literals, import type, import ordering ([9680888](https://github.com/sayanmohsin/thingd/commit/96808889f115659f5f2a5cca780ae8a84920a06a))

## [0.40.0](https://github.com/sayanmohsin/thingd/compare/v0.39.0...v0.40.0) (2026-06-29)

### Features

* cloud CLI commands with login, project, instance, api-key management ([828e979](https://github.com/sayanmohsin/thingd/commit/828e9790fa11d26849047afea5643dc25092b8ef))

### Bug Fixes

* update Cargo dependency version from 0.38 to 0.39 ([b037656](https://github.com/sayanmohsin/thingd/commit/b03765603ceb2d07fe82be6e91ddc193374c1f4a))

## [0.39.0](https://github.com/sayanmohsin/thingd/compare/v0.38.2...v0.39.0) (2026-06-28)

### Features

* **skill:** add audit-after-change skill — doc cross-ref, thingd-cloud sync, test gap check ([0958e12](https://github.com/sayanmohsin/thingd/commit/0958e12818a1e3f6bbd1fcb9fae02df0907677ee))

## [0.38.2](https://github.com/sayanmohsin/thingd/compare/v0.38.1...v0.38.2) (2026-06-28)

### Bug Fixes

* **cli:** writeJson for db subcommands, remove fragile internal state access in TUI ([7a7376e](https://github.com/sayanmohsin/thingd/commit/7a7376ef813acedcd399cfa1d5cfed4725f36519))

## [0.38.1](https://github.com/sayanmohsin/thingd/compare/v0.38.0...v0.38.1) (2026-06-28)

### Bug Fixes

* bump thingd version specifiers from 0.37 to 0.38 ([e3876f4](https://github.com/sayanmohsin/thingd/commit/e3876f45634c3b3844700ab1a3e7591ecf808fc0))
* **engine:** panic on clock error in debug, add Hash derives to model types ([d7321fd](https://github.com/sayanmohsin/thingd/commit/d7321fd115d60c53d06e5012fc6c19eaf3e8a14b))
* **sidecar:** rate limiter hardening and cluster status from real config ([1f9ba9c](https://github.com/sayanmohsin/thingd/commit/1f9ba9c9648d50e46cd415193ab6926e849fc0e9))

### Performance Improvements

* **engine:** push FTS collection filter and LIMIT to SQL, optimize delete_last_event ([8ef0e75](https://github.com/sayanmohsin/thingd/commit/8ef0e7550a21834609183c6c9649156a7f0626da))

## [0.38.0](https://github.com/sayanmohsin/thingd/compare/v0.37.11...v0.38.0) (2026-06-28)

### Features

* **sidecar:** implement all 27 MCP tools (was 5 stubs) ([e6f45e9](https://github.com/sayanmohsin/thingd/commit/e6f45e97fc32811f39a29e3cb3729dffbc783718))

### Bug Fixes

* add missing colon to docs site logo ([73e7ace](https://github.com/sayanmohsin/thingd/commit/73e7ace22fbba0726f7315a3ca1d095eb29a27aa))
* correct favicon href to include base path ([d8134aa](https://github.com/sayanmohsin/thingd/commit/d8134aae9ef5726038f06594d1e03432f5d68745))
* logo character spacing, navbar sizing, and copyright year ([e494f05](https://github.com/sayanmohsin/thingd/commit/e494f05e23a9d0fffb2e7fc784faa5579be605f1))
* match logo style to thingd-cloud (sans-serif, explicit sizing) ([ba03e24](https://github.com/sayanmohsin/thingd/commit/ba03e2482f327dfee46bdebc4c13b0d727c5777f))
* navbar logo size and aspect ratio ([e22508b](https://github.com/sayanmohsin/thingd/commit/e22508b1950f070b4efed85ef84a13e1cea8acd3))
* navbar logo width to prevent right-side clipping ([2ef3059](https://github.com/sayanmohsin/thingd/commit/2ef305955ba61ac0d5d3e0d299e80d71d75f3c89))
* prevent docs logo from clipping in navbar ([06b18cd](https://github.com/sayanmohsin/thingd/commit/06b18cd1fdffb9a350efc261659b3279e79554ce))
* remove extra spacing around docs logo braces ([2a4b157](https://github.com/sayanmohsin/thingd/commit/2a4b157350cda50e45a1c12817151c9ea94c5a07))
* remove whitespace between docs logo tspans ([4e83d39](https://github.com/sayanmohsin/thingd/commit/4e83d39a24679097dbde75bf0b268ab68f7c6cd4))
* rename QUICKSTART.md to quickstart.md (case-sensitive GitHub Pages), hide nav text, enlarge logo ([817ed0b](https://github.com/sayanmohsin/thingd/commit/817ed0bebf7af5c4637ddd6c44084781cd53f82a))
* rename QUICKSTART.md to quickstart.md for case-sensitive FS ([3c5102f](https://github.com/sayanmohsin/thingd/commit/3c5102fa3ec469b767039abe7902a6d2045cc2e1))
* rename QUICKSTART.md to quickstart.md, enlarge navbar logo ([a69f0f5](https://github.com/sayanmohsin/thingd/commit/a69f0f5218d08a7bc209274e6c1193377fe5463c))
* restore original logo sizing, update footer style ([857571b](https://github.com/sayanmohsin/thingd/commit/857571be80df5a3b769376314ca50067d415f1dc))
* **sdk:** concurrent batch ops, timeout, body size limit, error propagation ([1d802fe](https://github.com/sayanmohsin/thingd/commit/1d802fe5a3203d1c9f9c452a2dcf060efdd61954))
* **sidecar:** add graceful shutdown handler for SIGINT and SIGTERM ([1093989](https://github.com/sayanmohsin/thingd/commit/109398938db526a347d6e59223e98d3b32ee9a20))
* **sidecar:** constant-time auth, AppState token, production-mode MCP sanitization ([9a92d81](https://github.com/sayanmohsin/thingd/commit/9a92d81914edc17de0f9f652ec4b46c86746f6a6))
* **sidecar:** include object body in GET /v1/objects response ([e8557ed](https://github.com/sayanmohsin/thingd/commit/e8557ed9b7f50efb68680cf6c870ac134abcb807))
* **sidecar:** loud fallback on SQLite failure + track has_fallback status ([d16419e](https://github.com/sayanmohsin/thingd/commit/d16419e366e965c537d99fafe776a697a36ef285))
* **sidecar:** wire request_timeout_secs into tower timeout middleware ([96c6a9a](https://github.com/sayanmohsin/thingd/commit/96c6a9a84e6ce9be6c4cb4882a4d4372601b3e43))
* use monospace for even docs logo character spacing ([dc1326d](https://github.com/sayanmohsin/thingd/commit/dc1326d0baffb910e8b822fe04787cdcf9048922))
* use separate <text> elements to avoid } overlap in logo ([e0a53d1](https://github.com/sayanmohsin/thingd/commit/e0a53d18ab8d8a54169e26bc1e5cee5fd9cbb3c7))
* use tspan for logo text (no hardcoded x positions) ([6414fd4](https://github.com/sayanmohsin/thingd/commit/6414fd4be7c0c6a6cc54852e2e1922195e66d5e2))

## [0.37.11](https://github.com/sayanmohsin/thingd/compare/v0.37.10...v0.37.11) (2026-06-25)

### Bug Fixes

* inline particle component as h() function to prevent SSR hydration error ([98bcc60](https://github.com/sayanmohsin/thingd/commit/98bcc605ff9275b082dc52c0eac4ce5e73d5e235))
* inline particle component to prevent SSR hydration crash ([810a75d](https://github.com/sayanmohsin/thingd/commit/810a75d872c9740d9cfbfda9f310d278f808be72))
* remove duplicate hero logo, redesign SVG with gradients and better glow ([270b09f](https://github.com/sayanmohsin/thingd/commit/270b09ff0aa2e5cdbb51afa8d965f488dc83ef9e))

## [0.37.10](https://github.com/sayanmohsin/thingd/compare/v0.37.9...v0.37.10) (2026-06-25)

### Bug Fixes

* use allowBuilds in pnpm-workspace.yaml for docs deploy (pnpm 11 migration) ([d6ce40d](https://github.com/sayanmohsin/thingd/commit/d6ce40d01be4247ec63b85a883b11d6a155a541c))

## [0.37.9](https://github.com/sayanmohsin/thingd/compare/v0.37.8...v0.37.9) (2026-06-25)

### Bug Fixes

* remove hardcoded pnpm version from deploy-docs workflow (use packageManager from package.json) ([ad6ce6d](https://github.com/sayanmohsin/thingd/commit/ad6ce6de3b926142882d36790f3b7a030411d9ef))
* use pnpm install without --frozen-lockfile for docs deploy ([bce37ab](https://github.com/sayanmohsin/thingd/commit/bce37abcdf682bacda4a2c4ccc9361d9ad33e17f))

## [0.37.8](https://github.com/sayanmohsin/thingd/compare/v0.37.7...v0.37.8) (2026-06-25)

### Bug Fixes

* biome lint in docs theme (hoisted h(), import order) ([02ee53f](https://github.com/sayanmohsin/thingd/commit/02ee53f43dac043f2e00e5ec196b8433e3316cca))

## [0.37.7](https://github.com/sayanmohsin/thingd/compare/v0.37.6...v0.37.7) (2026-06-25)

### Bug Fixes

* move prebuild merge after pnpm install in release workflow ([a45bae3](https://github.com/sayanmohsin/thingd/commit/a45bae31362b81ddd1a34519f127d9cd47410aed)), closes [#39](https://github.com/sayanmohsin/thingd/issues/39)

## [0.37.6](https://github.com/sayanmohsin/thingd/compare/v0.37.5...v0.37.6) (2026-06-24)

### Bug Fixes

* test native filter on every build-native matrix runner ([7f0b178](https://github.com/sayanmohsin/thingd/commit/7f0b1784aa6cb56e02f8e3ebde73bc883c10df2d))
* use shell bash for native filter test on Windows runners ([10e916c](https://github.com/sayanmohsin/thingd/commit/10e916cf10f3d02551310b951b65a52df89fc282))

## [0.37.5](https://github.com/sayanmohsin/thingd/compare/v0.37.4...v0.37.5) (2026-06-24)

### Bug Fixes

* rebuild native prebuild with correct filter (cache key + heredoc test) ([b474a59](https://github.com/sayanmohsin/thingd/commit/b474a59f15b0c1269073708dc26957d73ff5bf83))

## [0.37.4](https://github.com/sayanmohsin/thingd/compare/v0.37.3...v0.37.4) (2026-06-24)

### Bug Fixes

* replace node -e with heredoc in CI workflows ([7c5d347](https://github.com/sayanmohsin/thingd/commit/7c5d3470cae976637fd43ddbe9fbcf20045ab654))
* use unique temp path for native filter test to avoid CI conflicts ([0a168b0](https://github.com/sayanmohsin/thingd/commit/0a168b0c94982b0ed7757f8f5f2c3a49bbf0a672))

## [0.37.3](https://github.com/sayanmohsin/thingd/compare/v0.37.2...v0.37.3) (2026-06-24)

### Bug Fixes

* eliminate all biome noExplicitAny warnings ([7b392ce](https://github.com/sayanmohsin/thingd/commit/7b392ce5fcf0044f7042b39f7c0b416503d104c1))

## [0.37.2](https://github.com/sayanmohsin/thingd/compare/v0.37.1...v0.37.2) (2026-06-24)

### Bug Fixes

* add ref:main to all release workflow checkouts ([42593f3](https://github.com/sayanmohsin/thingd/commit/42593f321a8dad6620717a0fe7298c6e3b824d90))
* disable lefthook during release to prevent push failures ([23dbd06](https://github.com/sayanmohsin/thingd/commit/23dbd0636374bf7b1abc3850d96f77ed696c770a))
* keep build-native dynamic name even when skipped ([5bbf0f2](https://github.com/sayanmohsin/thingd/commit/5bbf0f2c43fdf82ac412f911d85a11868e28a0d2))
* release check-release uses --format=%s not --oneline ([19a587f](https://github.com/sayanmohsin/thingd/commit/19a587f01d6796ace7f0f47448bc1b32b0ec1a92))
* skip pre-push hooks during semantic-release git push ([54d1b05](https://github.com/sayanmohsin/thingd/commit/54d1b055956856787120afc80e46a8026eff43ad))
* use static job name for build-native to avoid raw template in skipped matrix jobs ([99c0ec3](https://github.com/sayanmohsin/thingd/commit/99c0ec3788611912f9f8ded36d688fe71a18e31e))

## [0.37.1](https://github.com/sayanmohsin/thingd/compare/v0.37.0...v0.37.1) (2026-06-24)

### Bug Fixes

* cargo-deny config format for v0.18+ ([bc1cedc](https://github.com/sayanmohsin/thingd/commit/bc1cedce904c1a70e148515030eac44e49538969))
* NativeThingStore filter silently drops undefined values ([148d853](https://github.com/sayanmohsin/thingd/commit/148d853548b9984ae8dc3d7533e1d0a7b0a475df))

## [0.37.0](https://github.com/sayanmohsin/thingd/compare/v0.36.0...v0.37.0) (2026-06-24)

### Features

* implement all remaining improvements ([29a064f](https://github.com/sayanmohsin/thingd/commit/29a064f721899c905c82b5aff29c7a7ab0d93e4f))

### Bug Fixes

* cargo fmt, biome lint, and pre-existing warnings ([8acbb28](https://github.com/sayanmohsin/thingd/commit/8acbb288e832db9e723fbf0bcb908b454a4f0f97))
* dashboard package.json path resolution + add test-node to pre-push ([35ee66c](https://github.com/sayanmohsin/thingd/commit/35ee66cf136373976f07a504db23cd90a7c29472))
* InMemoryThingStore.search silently drops options.filter ([30b0fdd](https://github.com/sayanmohsin/thingd/commit/30b0fdd0e59f98e57554c0a83bff469c6f969f99)), closes [#39](https://github.com/sayanmohsin/thingd/issues/39)

## [0.36.0](https://github.com/sayanmohsin/thingd/compare/v0.35.0...v0.36.0) (2026-06-23)

### Features

* CLI db subcommand, dashboard health tab, security/operations docs ([7ae5970](https://github.com/sayanmohsin/thingd/commit/7ae597042fab33d08b4fdb7d599374e09e1044a8))

## [0.35.0](https://github.com/sayanmohsin/thingd/compare/v0.34.0...v0.35.0) (2026-06-23)

### Features

* atomic restore and backup-before-migration ([fd03859](https://github.com/sayanmohsin/thingd/commit/fd038591f5814c73135507b4e9fc022cb3ab48c4))
* rate limiting with token bucket middleware ([328b974](https://github.com/sayanmohsin/thingd/commit/328b97484466084f8ecf9c516715ca67a2c1f5bb))

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
