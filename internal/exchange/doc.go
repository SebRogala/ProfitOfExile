// Package exchange reads Path of Exile's public currency exchange feed, turns one
// published hour into normalized market rows, and stores the active league's rows
// on an hourly cursor walk.
//
// The feed is GET https://web.poecdn.com/api/currency-exchange[/{realm}][/{id}]
// where {id} is a unix hour. Observed 2026-08-19: no auth, no ETag, no Age and
// no rate-limit headers; a published hour answers 200 with roughly 1.7 MB of
// JSON, an unpublished (current or future) hour answers 404 with the body
// {"next_change_id":<current unpublished hour>,"markets":[]}, and an omitted id
// answers 200 with the oldest retained hour.
//
// next_change_id is a cursor, not the hour the body describes: on a 200 it names
// the following hour (hour 1787119200 answered 1787122800), and on a 404 it names
// the hour to retry.
//
// # Layers
//
// client.go, payload.go, normalize.go and humanize.go are pure: no database, no
// scheduler, no HTTP server, no collector. Callers fetch a payload with
// Client.FetchHour and derive rows with Normalize; Normalize, PriceOf and Ratio
// are the only places prices are computed.
//
// repository.go and runner.go are the lifecycle layer — database access and the
// ticking cursor walk — mixed into the same flat package the way
// internal/collector mixes fetcher, repository and scheduler
// (docs/adr/008-current-go-package-architecture.md). The layering rule the
// compiler cannot enforce: this package must never import internal/collector.
// The Mercure stamping adapter that needs both lives in cmd/collector.
//
// # Storage
//
// Two league-scoped tables, created by
// internal/db/migrations/20260819120000_create_currency_exchange_markets.up.sql:
//
//   - currency_exchange_markets — one row per (league, feed hour, market), keyed
//     (league, time, market_id) on a hypertable partitioned by time. `time` is the
//     feed hour itself in UTC, truncated, not the moment of collection. Seventeen
//     columns: the fifteen persisted Row fields plus the league and the hour.
//     Row.LowestPriceBInA and Row.HighestPriceBInA are NOT stored, because
//     consumers derive them with Ratio and a stored float would be a second
//     source of truth.
//   - currency_exchange_cursor — one row per league holding next_hour, the unix
//     hour the runner fetches next.
//
// Both tables carry a compression policy and deliberately no retention policy
// (docs/adr/010-archived-league-history-is-retained-indefinitely.md), and both are
// in the set truncated at league rollover
// (docs/adr/011-wipe-the-outgoing-league-at-rollover-preserve-it-as-a-dump.md).
//
// # Cursor
//
// The cursor is a dedicated row rather than MAX(time) over the markets table. An
// hour whose markets all belong to other leagues stores zero rows, and MAX(time)
// would make the walk re-fetch that 1.7 MB hour on every tick instead of moving
// past it. The cursor advances in the SAME transaction as the hour's inserts, so a
// crash mid-hour leaves the cursor on the unfinished hour and the retry is
// idempotent against the primary key. With no cursor row the walk bootstraps at
// now−24h truncated to the hour.
//
// # Events
//
// The runner publishes one Mercure event per stored hour — including an hour that
// stored zero rows, so downstream recompute knows the hour passed — on topic
// poe/collector/currency-exchange, with the fields topic, endpoint, hour,
// nextCursor, rows and timestamp, plus the league and leagueRevision stamp that
// collector.StampScope adds (the server drops unstamped events).
//
// rows counts the rows the publishing pass INSERTED, not the rows the hour holds:
// an hour replayed after a crash re-inserts nothing and publishes rows: 0 even
// though the hour is fully populated. Consumers must read the field as "how much
// is new" and reach for Repository.LoadRows when they need the hour's contents.
//
// # Configuration
//
// EXCHANGE_INGEST_ENABLED gates the runner and defaults to ON; only the exact
// value "false" disables it. This inverts TRADE_ENABLED's default-off idiom on
// purpose: the ingest walk is the only writer of the hourly history, and a missed
// window is not backfillable past the feed's retention. EXCHANGE_TICK sets the
// tick interval and defaults to 5m. EXCHANGE_PER_HOUR_DELAY paces the hours
// within one catch-up pass and defaults to 250ms; 0 disables the pacing, and the
// default is applied by the cmd/collector wiring rather than by NewRunner.
//
// Naming rule: the Go package is named exchange, and every user-facing string
// (User-Agent, error text, log message) says "currency-exchange". A bare "cx"
// identifier is never used.
package exchange
