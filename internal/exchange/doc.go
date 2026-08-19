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
// client.go, payload.go, normalize.go, humanize.go and the four engine files
// pricing.go, direct.go, crossquote.go and plays.go are pure: no database, no
// scheduler, no HTTP server, no collector. Callers fetch a payload with
// Client.FetchHour and derive rows with Normalize; Normalize, PriceOf, Ratio and
// priceIn are the only places prices are computed.
//
// repository.go and runner.go are the lifecycle layer — database access and the
// ticking cursor walk — mixed into the same flat package the way
// internal/collector mixes fetcher, repository and scheduler
// (docs/adr/008-current-go-package-architecture.md). The layering rules the
// compiler cannot enforce: this package must never import internal/collector,
// and never internal/lab either — the engine scores the currency feed on its own
// terms rather than borrowing the gem stack's scoring vocabulary. The Mercure
// stamping adapter that needs both lives in cmd/collector.
//
// # Engine
//
// BestPlays (plays.go) is the single entry point: it takes a league's StoredRows
// and returns a ranked Result. It finds two shapes of opportunity per feed hour.
//
// A direct flip buys and sells the same item on one market:
//
//	edge = high/low - 1
//
// A one-hop triangle buys item X against currency A, sells it against currency
// B, and converts the proceeds back on the A/B market — three executed legs:
//
//	edge = highXinB * highBinA / lowXinA - 1
//
// A cross-quote play is one item against two quote currencies, and the
// currencies are exactly the ids in Config.QuotePriority: A and B must both be
// listed and X must not be. A triangle of three non-currency items is not a
// 1-hop play, however well its three markets close, and no route ever trades a
// quote currency as its item. An item quoted in both default currencies
// therefore yields two routes — a direction and its mirror — not the three
// rotations of its triangle.
//
// Every price comes from priceIn (pricing.go), the one place that maps a stored
// quantity pair onto a direction; it hands the quantities to Ratio rather than
// dividing, and the engine never computes 1/price from a stored float, which
// would be a division by zero on an unpriced row.
//
// The caveat that governs every edge: the feed publishes each hour's realized
// LOW and HIGH, not a book. The two prices are trades that happened during the
// same hour, not two sides that stood at the same instant, so an edge is an upper
// bound on what was takeable — and on items that move in one-or-two-divine ticks,
// most of the apparent spread is quantization rather than opportunity.
//
// Two filters push back on that. Each leg is gated on the hour's traded volume
// and stock, so an edge nobody could have executed never becomes a play. The
// floor applies to the ITEM side of the leg as oriented by Config.QuotePriority,
// which means a currency/currency market gates on the lower-priority currency —
// chaos in a chaos/divine market under the default priority — and the epic's
// "59% keep-rate" measurement used min(volA, volB) and so does not describe this
// gate.
//
// Config.MinHoursSeen is the ghost filter: an edge that printed in a single hour
// of the window and never repeated is far more likely to be a digest artifact
// than a standing opportunity, so a play must survive several hours to be
// returned. Aggregation weights recent hours more heavily (w_i = (N - i) / N for
// the i-th newest of N hours), which is what keeps one stale hour from carrying a
// play.
//
// Play.Depth carries a known comparability caveat: for a direct play it is the
// item's FULL hourly volume on the market even though the recipe both buys and
// sells that item, so a round trip can absorb at most half of it, and a direct
// Depth is not on the same scale as a 1-hop Depth. Whether to halve it is
// POE-178/180's call, not something to change silently.
//
// What the Result contract promises its consumers (POE-175/176): From is the
// oldest hour PRESENT in the window and To the newest hour plus one, so a gap in
// the feed makes the span wider than Config.WindowHours while Hours stays the
// honest count of hours that carried data. An empty result carries zero From/To
// rather than a real interval, which the handler must special-case instead of
// rendering. Keys are opaque: a MarketID contains "|" itself, so a key must
// never be parsed — read Play.Mode and Play.Legs, which carry the same facts in
// typed fields. A direct play's two legs are the SAME market and the SAME item
// (buy low, sell high); a 1-hop play's three legs are three markets in execution
// order. And a change to Config.QuotePriority leaves direct Keys untouched while
// flipping which side of each leg reads as Item and which as Quote.
//
// Nothing here is persisted: BestPlays is recomputed from stored rows. Storing
// plays, and the stricter filtering that only makes sense once they can be
// compared against their own history, is POE-180. Results carry raw feed item
// ids; the HTTP handler is what runs them through Humanize.
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
//
// # Operations
//
// One failure mode needs a hand: the walk can be left on an hour the feed will
// never serve. When the 404 body's next_change_id is more than one hour ahead of
// the cursor, the hours in between are not coming — aged out of retention, or
// never published — and RunOnce stops without advancing and logs
//
//	WARN currency-exchange: feed moved past the cursor hour=… nextChangeID=… gapHours=…
//
// on every tick. The cursor does not move on its own from there. Read its
// position with
//
//	SELECT league, next_hour, to_timestamp(next_hour) FROM currency_exchange_cursor;
//
// and move it forward by hand:
//
//	UPDATE currency_exchange_cursor
//	SET next_hour = <unix hour to resume from>, updated_at = now()
//	WHERE league = '<league>';
//
// Resume at the WARN's nextChangeID minus 3600 — the newest complete hour — or at
// any earlier hour the feed still serves; the walk re-fetches from there and the
// inserts are idempotent against the primary key, so overlapping an already
// stored range costs bandwidth and nothing else. The collector does not need a
// restart: the next pass reads the cursor row again.
package exchange
