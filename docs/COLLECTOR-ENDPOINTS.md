# Adding a Collector Endpoint

**Status:** Current  
**Last verified:** 2026-07-22  
**Canonical for:** Cross-layer procedure for adding a collector-backed market-data endpoint.

Use the existing fragments endpoint as the working reference. Exact fields,
constraints, retention, and compression policy depend on the source; copy the
project's integration pattern, not its schema blindly.

1. Define the source identity and storage schema. Create a new timestamped
   migration pair with `make migration name=descriptive_name`; never edit a
   migration that may have been deployed. Follow the nearest current hypertable
   for keys, indexes, compression, and retention, then justify any differences.
2. Add the typed snapshot model in `internal/collector/fetcher.go`.
3. Extend `FetchResult` in `internal/collector/endpoint.go` with the new typed
   slice and update `Validate` so only one data variant can be populated.
4. Add the canonical endpoint-name constant in `internal/collector/endpoint.go`.
5. Implement the upstream response conversion and fetch method in
   `internal/collector/ninja.go`, or in a source-specific client when the data
   does not come from poe.ninja.
6. Extend the repository interface and implement the endpoint's latest-snapshot
   and parameterized batch-insert methods in `internal/collector/repository.go`.
7. Add the endpoint-to-topic mapping in `mercureTopicSuffix` in
   `internal/collector/scheduler.go`.
8. Wire `FetchFunc`, `StoreFunc`, and `StalenessFunc` in
   `cmd/collector/main.go`, then add the configuration to the scheduler's
   endpoint list. Preserve `FetchResult.Validate` checks at construction and
   consumption boundaries.
9. Add snapshot handlers and routes under `internal/server`; update aggregate
   snapshot/status responses when the endpoint belongs there.
10. If the server reacts to this endpoint's Mercure notification, subscribe to
    its topic in `cmd/server/main.go` and handle the canonical endpoint name.
11. Add focused tests for conversion, result validation, repository behavior,
    scheduling/topic publication, and HTTP output as applicable. Read the
    repository's test-author contract before creating or modifying tests.

The current fragments path spans `internal/collector/{fetcher,endpoint,ninja,
repository,scheduler}.go`, `cmd/collector/main.go`, `internal/server`, and
`cmd/server/main.go`. Search by `EndpointNinjaFragments`, `FragmentData`, and
`poe/collector/fragments` to inspect the complete implementation before editing.
