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
