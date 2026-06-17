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
