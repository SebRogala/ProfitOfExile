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
// client.go, payload.go, normalize.go, humanize.go, items.go and the four engine files
// pricing.go, direct.go, crossquote.go and plays.go are pure: no database, no
// scheduler, no HTTP server, no collector. Callers fetch a payload with
// Client.FetchHour and derive rows with Normalize; Normalize, PriceOf, Ratio and
// pricing.go's direction mappers — priceIn, vwapIn, tickOf and the volume
// readers — are the only places prices are computed.
//
// repository.go, runner.go and service.go are the lifecycle layer — database
// access, the ticking cursor walk, and the server-side recompute that keeps the
// served answer current — mixed into the same flat package the way
// internal/collector mixes fetcher, repository and scheduler
// (docs/adr/008-current-go-package-architecture.md). The layering rules the
// compiler cannot enforce: this package must never import internal/collector,
// and never internal/lab either — the engine scores the currency feed on its own
// terms rather than borrowing the gem stack's scoring vocabulary. The Mercure
// stamping adapters that need both live in cmd/collector and cmd/server.
//
// # Engine
//
// BestPlays (plays.go) is the single entry point: it takes a league's StoredRows
// and returns a ranked Result. It works in two passes — each feed hour is scored
// on its own, and the surviving candidates are then merged by recipe key, where
// every number a Play shows — except Stock, RoiPctNewestHour, HoursSeen and
// LastHour — is a cross-hour MEDIAN of those per-hour readings or arithmetic on
// such medians.
//
// What one hour observes about one leg (the obs struct in direct.go, filled by
// gatedLeg): the hour's cheapest and dearest realized price of the item in its
// quote (priceIn); the volume-weighted average price the hour's mass actually
// cleared at, quote units traded divided by item units traded (vwapIn); the
// market's price resolution (tickOf — the feed quotes each side as a reduced
// integer quantity pair, so the smallest representable step on a pair (a, b) is
// 1/max(a, b), and the coarser of the row's two pairs bounds everything derived
// from it); the traded units of both sides; and the item's highest stock. A leg
// counts for the hour only when priceIn can price it, at least
// Config.MinVolumePerHour units of the ITEM side traded, and both sides carried
// stock. That gate is liveness, not liquidity, and one failed leg drops the
// whole candidate — a recipe is only as executable as its thinnest step.
//
// Two shapes of candidate come out of each hour. A direct flip buys and sells
// the same item on one market:
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
// would be a division by zero on an unpriced row. vwapIn, tickOf and
// quoteVolumeOf sit beside it for the same reason.
//
// The merge is a median with NO recency weight: hours are equal citizens,
// because the failure mode being defended against is a single hour's blowup and
// 38% of markets print an hourly edge above five times their own median. Per leg
// a Play carries Price — the median of the hourly LOWS on a buy leg and of the
// hourly HIGHS on a sell leg, because a market maker posts at the edges and the
// extreme is the executable recipe — Fair, the median VWAP over the hours that
// HAD one (an hour whose quote side reported no volume is skipped rather than
// averaged in as a zero, and a leg no hour ever priced reads 0, meaning "no
// anchor"), median Tick, median item Volume, and Stock from the NEWEST hour
// only. Stock is liveness and nothing else: the feed's stock columns are the
// hour's min and max of total book size and say nothing about the extreme
// (measured corr <= 0.13 against the edge).
//
// The play-level numbers are arithmetic on those emitted legs, which is what
// makes a Play reproducible from what it shows:
//
//   - RoiPct is recomputed FROM the leg Prices: sell/buy − 1 for a direct flip,
//     (sellXinB * sellBinA)/buyXinA − 1 for a triangle. Edge is the same value
//     under its pre-POE-184 name, kept for clients written before the rename and
//     deprecated in favour of RoiPct.
//   - RoiPctNewestHour is LastHour's own extreme-to-extreme edge — the NOW
//     reading, not a bound: it sits above RoiPct when the newest hour was louder
//     than the window's typical one and BELOW RoiPct when that hour was quiet.
//   - Investment is the first leg's Price valued in chaos, and Roi is the chaos
//     one exchanged unit returns. Roi == RoiPct * Investment by construction,
//     not two independent measurements.
//   - Turnover is the chaos per hour flowing through the play's THINNEST leg:
//     the minimum over legs of median quote-side volume times that quote's chaos
//     rate. It is the liquidity reading, and unit volume is not one.
//   - Tick is the MAXIMUM over the legs' median ticks — the worst step the
//     recipe has to live with: the one market for a direct flip, the worst of
//     three for a triangle.
//   - Depth is the minimum over the legs' median item Volume. It carries a
//     comparability caveat: for a direct play it is the
//     item's FULL hourly volume even though the recipe both buys and sells that
//     item, so a round trip can absorb at most half of it, and a direct Depth is
//     not on the same scale as a 1-hop Depth. Whether to halve it is POE-180's
//     call, not something to change silently; Turnover already measures the
//     quote side.
//
// Chaos is the unit every absolute number is expressed in, through one
// window-level scalar: Result.DivineChaosRate, the median across the window's
// hours of the divine/chaos market's VWAP. It is measured from the same table as
// everything else (198.97 c/div over the reference window) rather than
// hard-coded, and it is per window rather than per play so that two divine-quoted
// plays are valued identically. Chaos-quoted legs convert at 1, divine-quoted
// legs at that rate. A play whose quote is neither chaos nor divine —
// scarab/scarab, card/scarab and every other cross-item market — is dropped
// outright, because gates denominated in chaos cannot judge a payout measured in
// scarabs. When the divine/chaos market did not trade in any of the window's
// hours the rate is 0 and every divine-quoted play is dropped rather than valued
// at a guess; that is logged once per HORIZON, so twice per Service recompute.
//
// The gates, in the order play() applies them, with DefaultConfig's values:
// HoursSeen >= MinHoursSeen (2 on the base config, capped at the hours actually
// present so a short window still returns plays, and overridden per horizon),
// RoiPct >= MinEdge (0.02), Turnover >= MinTurnoverChaos (10,000 chaos/hour),
// Tick <= MaxTick (0.10), RoiPct >= MinEdgeTickRatio * Tick (5 steps), and
// Roi >= MinROIChaos (3 chaos per exchanged unit). MinVolumePerHour (10 units)
// has already run, per leg per hour.
//
// Those LEVELS come from 30,534 priced Allflame market-hours: price quantization
// is the strongest single predictor of an apparent spread (corr(ln edge,
// ln tick) = +0.42, median tick 14.3%), which is what MaxTick and
// MinEdgeTickRatio answer, and chaos-denominated flow predicts a real edge where
// unit volume does not (−0.30 against +0.06; p50 robust edge 242% under 100
// chaos/hour and 18% over 100k), which is what MinTurnoverChaos answers. The
// levels were calibrated against that measurement's statistic — the median of
// the per-hour edges, under which the set took 908 markets to 135 — while the
// engine gates on the related ratio of the medians, so 908→135 is a calibration
// of the levels and not a prediction of how many plays come back; the served
// ranking was confirmed by a live check against the running stack.
//
// Ranking is RoiPct desc, then Turnover desc, then direct before 1-hop (one
// execution risk instead of three), then Key ascending, truncated to MaxPlays
// (100). It is a stable sort over a key-sorted list, so identical rows produce
// identical output whatever order they arrived in.
//
// The caveat that governs every percentage has two layers. First, the feed
// publishes each hour's realized LOW and HIGH, not a book: the two prices are
// trades that happened during the same hour, not two sides that stood at the
// same instant. Second, the medians are synthesized ACROSS hours — even a direct
// play's two Prices are independent medians, one over the hourly lows and one
// over the hourly highs, so its RoiPct is a combination no single hour
// necessarily offered, and a 1-hop play compounds that over three such medians
// AND three separate markets. A Play is the typical shape of a route, not a
// trade someone made. The tick gates are what bound the fiction, Fair is what
// anchors it, and RoiPctNewestHour is the one number that belongs to a single
// real hour.
//
// Volume is a per-side TOTAL, not a split by direction: the feed publishes
// volume_traded for each side of a market and says nothing about how much of it
// was bought versus sold, so no number here distinguishes the two.
//
// BestPlays ranks ONE window; which window is the Service's call. It runs the
// engine once per HorizonConfig (service.go) over the same loaded rows: recent,
// six hours needing four, which is what a request naming no horizon gets, and
// day, twenty-four hours needing eighteen. Both hours-seen demands are about
// three quarters of their window and in absolute terms ask for very different
// things, which is the point — a recent play may be two hours old, a day play
// may not. A horizon overlays Config.WindowHours and Config.MinHoursSeen and
// nothing else; BestPlays ignores Config.Horizons itself.
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
// ids; the HTTP handler is what turns them into display names and icon paths,
// through the resolver below.
//
// # Items
//
// The feed sends metadata ids and nothing else — no display name, no icon — so
// both are carried in the binary. itemdata/items.json is a committed asset,
// `{"<metadata id>": {"name": ..., "icon": <poewiki URL> | null}}` sorted by id,
// embedded by items.go and parsed once at package init. A malformed asset
// panics there: it is a build artifact, so the defect is in the committed file
// and is the same in every process.
//
// items.go is the whole resolver. LookupItem is the raw read; DisplayName is
// what callers want, returning the asset name and falling back to Humanize for
// an id the asset does not cover. Humanize is the FALLBACK, not the mechanism —
// it splits an id's CamelCase tail, which is readable but often wrong (it calls
// a Chaos Orb "Reroll Rare"), and it exists so an id added since the last
// regeneration still renders. UnknownItems.Note records such an id with one
// Warn per distinct id rather than one per occurrence, because an unknown id
// recurs in every leg of every play that touches it on every recompute.
//
// Icons are not served from poewiki: it 403s the production VPS
// (docs/adr/012-icons-are-pre-seeded-from-an-allowed-ip-and-cached-by-content-address.md),
// so the server serves its own cached copy through internal/gemicon's cache the
// way gem icons already work. IconURLs hands that cache a fresh id → upstream
// URL map, and IconPath returns the API-relative client path
// "/currency-exchange/icon/<escaped id>" — the id escaped as a SINGLE path
// segment, so its slashes are %2F — or false for an item with no icon, which
// renders without one rather than requesting a URL that would 404 every time.
// The prefix carries no "/api" because clients join it onto a base that already
// ends in one.
//
// Regeneration, once per league (GGG adds items between leagues, not within
// one), from the repository root:
//
//	python3 scripts/generate-currency-exchange-items.py
//
// It reads the id universe from the RePoE-fork base-item dump filtered to the
// eight exchange categories, joins poewiki's items cargo table by metadata id
// for the icon File names, resolves the distinct Files to their poewiki image
// URLs fifty at a time through the imageinfo API — a request per file
// instead of per fifty earns a 429 partway through, measured 2026-08-19 — and
// prints coverage per category. Those URLs are committable because MediaWiki
// derives the /images/<h>/<hh>/ path from the MD5 of the File NAME, so a
// re-upload under the same name keeps the URL. The whole run is about thirty
// requests. It refuses to write on a name-coverage shortfall or a cache-filename collision,
// and its output is deterministic, so an unchanged upstream re-runs to a
// zero-length diff. Unnamed drop-table placeholders (RePoE carries hundreds of
// RandomFossilOutcome<N> entries in the Currency namespace) are excluded rather
// than shipped under a fabricated name. It also writes itemdata/icon-urls.json,
// a flat id → URL map, which is what scripts/download-gem-icons.py pre-seeds
// the production icon cache from.
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
// # Server surface
//
// service.go is the read side: the collector writes hours, the server recomputes
// from them and serves the answer out of memory. Nothing is persisted — the
// plays are derived, and storing them is POE-180.
//
// Cache holds the newest Result PER HORIZON and reports warmth as an explicit
// flag, per the cache-state contract in internal/lab/cache.go. COLD (no
// recompute has stored anything for that horizon yet) and WARM-AND-EMPTY (the
// recompute ran and honestly found no plays) hold the same empty value, so a
// caller must never infer warmth from what it read; Service.Recompute stores its
// answer even when it is empty, which is the writer half of the same rule. Each
// horizon warms on its own, so Cache.Snapshot takes the horizon to read and its
// answer says nothing about the other one — though in practice a recompute
// writes both before it returns. Snapshot hands back a copy whose Plays slice is
// fresh, so a handler may filter it in place.
//
// Service.Recompute anchors its window on the newest hour that HAS rows, not on
// the clock and not on the ingest cursor: it loads [newest − widest·1h,
// newest + 1h) ONCE — widest being the longest span any configured horizon
// ranks — and then runs BestPlays over that same slice once per horizon,
// because the engine keeps only the newest WindowHours distinct hours it is
// given. Two horizons therefore cost two in-memory passes and one hypertable
// scan, and both describe the same instant of the feed. The read reaches one
// clock hour further back than the widest window on purpose: a feed that missed
// a poll would otherwise rank an hour short. Recompute returns the FIRST
// configured horizon's result, which is what an unqualified request is served.
// A stalled feed keeps serving its last real hours instead of sliding into an
// empty window. A league with no rows warms every horizon with an empty result
// rather than erroring. On any error the cache is left untouched and nothing is
// signalled, so a failed read never downgrades a good cached answer.
//
// Service.Trigger coalesces: while a recompute is in flight, further triggers
// only mark the service dirty, and the running recompute repeats once for
// however many arrived — a six-hour catch-up pass costs as few as two window
// reads. How many it actually costs depends on how a recompute's duration lines
// up against the collector's EXCHANGE_PER_HOUR_DELAY pacing: triggers that
// arrive while one is in flight coalesce, ones that arrive between runs do not. Service.HandleEvent ignores the event payload entirely, which is what
// makes it replay-safe: the collector's "rows" field counts what that pass
// INSERTED, so a replayed hour reports rows: 0 while being fully populated, and
// a content check would skip exactly the recompute a crash recovery needs. The
// league guard is the caller's job (server.LeagueEventGuard).
//
// Debouncer collapses a burst of signals into one trailing-edge call after
// DefaultUpdateDebounce (2s) of quiet. internal/lab/throttler.go is the same
// idea and is NOT reused: it hard-codes the topic poe/analysis/updated, holds a
// *lab.Cache to write SetNextFetch into, and builds a gem-stack payload — and
// reaching for it would import internal/lab, which the boundary test forbids.
//
// After each recompute the server publishes UpdatePayload on
// poe/currency-exchange/updated (UpdatedTopic), debounced. That topic is
// deliberately not the collector's poe/collector/currency-exchange: one says
// "an hour was ingested", the other "the served answer changed", and a client
// listening to the ingest topic would refetch before the recompute had run. The
// payload carries topic, league, lastUpdated, hours, plays (a COUNT, not the
// plays) and timestamp, plus the league and leagueRevision stamp
// collector.StampScope adds in cmd/server. It is a notification: clients refetch
// the endpoint below for the data.
//
// lastUpdated is LastUpdated(Result) — Result.To minus one hour, i.e. the newest
// feed hour the answer covers, and null when it covers none. It is DATA
// freshness, not compute freshness: a recompute over unchanged rows leaves it
// where it was, where a wall clock would read fresh while the feed was hours
// stale.
//
// The endpoint (internal/server/handlers):
//
//	GET /api/currency-exchange/plays?mode=all|direct|1-hop&horizon=recent|day
//
// Both parameters are optional and default to all and recent; an unknown value
// of either is a 400 rather than a silent fallback, because a typo that quietly
// returned every play, or the other horizon's ranking, would look like a working
// filter. Picking a horizon is a map lookup, not work: both were computed by the
// same recompute.
//
// The response carries league, lastUpdated (RFC3339 or null), from, to, hours,
// warm, mode, horizon (echoed, so a cached body cannot be mistaken for the other
// window's), divineChaosRate (0 when the divine/chaos market did not trade in
// the window, in which case no divine-quoted play is in the list), count and
// plays. Each play carries key, mode, legs, roiPct, edge (its deprecated alias),
// roiPctNewestHour, roi, investment, turnover, tick, depth, hoursSeen and
// lastHour; each leg action, item, quote, price, fair, tick, volume and stock.
// A COLD cache answers 200 with an empty plays list, warm: false and
// lastUpdated: null rather than an error or a database fallback — the recompute
// is the only reader, so a fallback query would just repeat it. The handler
// never touches the database.
//
// Each leg gains four transport-only fields; the engine itself never carries
// display data. itemName and quoteName come from DisplayName (the asset, with
// Humanize as the fallback for an id it does not cover, noted once through
// UnknownItems), and itemIcon and quoteIcon from IconPath — API-relative paths
// into this server's icon route, or null for an item with no artwork, which the
// client renders without one. The icons are served by
//
//	GET /api/currency-exchange/icon/{escaped metadata id}
//
// which is internal/gemicon's cache over IconURLs() and its own cache directory
// (CURRENCY_EXCHANGE_ICON_CACHE_DIR, default ./data/currency-exchange-icons-cache;
// a persistent volume in production). An id absent from the map is a 404 and an
// unfetchable upstream a 502, exactly as for gem icons.
//
// The server reads its tuning from the environment in cmd/server, each override
// falling back to DefaultConfig on an unparseable value with a Warn:
// EXCHANGE_MIN_VOLUME_PER_HOUR, EXCHANGE_MIN_EDGE (may be negative),
// EXCHANGE_MAX_PLAYS and the four gate knobs EXCHANGE_MIN_TURNOVER_CHAOS,
// EXCHANGE_MAX_TICK, EXCHANGE_MIN_EDGE_TICK_RATIO and EXCHANGE_MIN_ROI_CHAOS.
// The window knobs are per horizon: EXCHANGE_RECENT_WINDOW_HOURS /
// EXCHANGE_RECENT_MIN_HOURS_SEEN and EXCHANGE_DAY_WINDOW_HOURS /
// EXCHANGE_DAY_MIN_HOURS_SEEN. EXCHANGE_WINDOW_HOURS and
// EXCHANGE_MIN_HOURS_SEEN still work but bind the RECENT horizon only — they
// leave the day window alone, and an EXCHANGE_RECENT_* name set alongside one of
// them wins. The Warn is louder
// than the silent fallbacks the collector's knobs use on purpose: a typo in a
// ranking threshold changes what users see and has no other symptom.
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
