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
// BestPlays (plays.go) is the single entry point: a league's StoredRows in, a
// ranked Result out, no database and no clock. It scores HOURS, not windows.
// Rows are grouped by feed hour, the newest Config.WindowHours distinct hours are
// kept, and each of those hours is scored ALONE — candidates built from that
// hour's rows, and evaluate turning each candidate into a finished Play (leg
// prices, undercut return, chaos payout, every gate) out of that one hour. Hours
// never mix. The merge by recipe key keeps exactly two things: the NEWEST cleared
// hour's Play, which is what gets served, and a COUNT of the hours that cleared,
// which becomes HoursSeen.
//
// A play is served only when it cleared in the window's NEWEST hour — the last
// snapshot — so Play.LastHour is that hour for every play in the Result, and a
// recipe that held four hours ago and has not held since is absent rather than
// shown at a price nobody can act on. The check compares against the window's
// newest hour rather than against whatever arrived last, so the answer does not
// depend on the order rows come in.
//
// Prices are one hour's because a cross-hour median served Mawr Blaidd/Chaos as
// "buy at 80.50" against a VWAP near 250 (POE-188).
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
//	roiPctRaw = high/low - 1
//
// A one-hop triangle buys item X against currency A, sells it against currency
// B, and converts the proceeds back on the A/B market — three executed legs:
//
//	roiPctRaw = highXinB * highBinA / lowXinA - 1
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
// A Play's legs are that one hour's observation, unaveraged. Price is the hour's
// LOW on a buy leg and its HIGH on a sell leg — a market maker posts at the
// edges, and the extreme is the executable recipe. Fair is the hour's VWAP, with
// FairOK saying whether the hour had one at all: an hour whose quote side
// reported no volume carries Fair 0, which means "no anchor" and not "free".
// Tick, Volume and Stock are the same hour's. Stock is liveness and nothing else:
// the feed's stock columns are the hour's min and max of total book size and say
// nothing about the extreme (measured corr <= 0.13 against the edge).
//
// The headline percentage is not computed at those prices. An order resting at
// exactly the last realized price sits behind everything already queued there, so
// every leg is undercut by one of its OWN ticks inside the arithmetic — buy at
// Price*(1+Tick), sell at Price*(1-Tick) — while the leg keeps the raw number, so
// the reader can check it against the game and rebuild the undercut from Tick.
// The play-level numbers are arithmetic over those legs, which is what makes a
// Play reproducible from what it shows:
//
//   - RoiPct is the round trip at the UNDERCUT prices: sell/buy − 1 for a direct
//     flip, (sellXinB * sellBinA)/buyXinA − 1 for a triangle. It is what the
//     gates and the ranking judge, because it is the return an order that
//     actually gets taken can expect. Edge is the same value under its
//     pre-POE-184 name, kept for clients written before the rename and deprecated
//     in favour of RoiPct.
//   - RoiPctRaw is the same round trip at the RAW extremes — what the hour
//     printed, with nothing paid for the fill. It never sits below RoiPct, and
//     the gap between the two is what the ticks cost: on a coarse market that gap
//     is the whole spread.
//   - Investment is the first leg's UNDERCUT entry price valued in chaos, and Roi
//     is the chaos one exchanged unit returns. Roi == Investment * RoiPct by
//     construction — both sides priced at the same entry, not two independent
//     measurements.
//   - Turnover is the chaos per hour flowing through the play's THINNEST leg: the
//     minimum over legs of the hour's quote-side volume times that quote's chaos
//     rate. It is the liquidity reading, and unit volume is not one.
//   - Tick is the MAXIMUM over the legs' ticks — the worst step the recipe has to
//     live with: the one market for a direct flip, the worst of three for a
//     triangle.
//   - Depth is the minimum over the legs' item Volume. It carries a comparability
//     caveat: for a direct play it is the item's FULL hourly volume even though
//     the recipe both buys and sells that item, so a round trip can absorb at most
//     half of it, and a direct Depth is not on the same scale as a 1-hop Depth.
//     Whether to halve it is POE-180's call, not something to change silently;
//     Turnover already measures the quote side.
//
// Suspect is the junk flag, and it flags rather than substitutes. A buy leg is
// suspect when its low sits under Fair*Config.SuspectLowBand (0.67), a sell leg
// when its high sits over Fair*Config.SuspectHighBand (1.5), and a leg whose
// FairOK is false is never suspect because there is no anchor to judge it
// against. The bands come from the same feed read the other way round: across 221
// liquid chaos markets over 24 hours an hour's extremes sit 11-13% off that
// hour's VWAP at p50 and 50% at p90. 1.5 sits at the p90 of ordinary noise and
// 0.67 inside it, tighter on the low side because that is where the junk prints.
// A Play is Suspect when ANY leg is, and it is still SERVED — flagged, and
// ranked after every clean play, because a flagged row can be argued with and a
// missing one cannot. Config.HideSuspect (default false, EXCHANGE_HIDE_SUSPECT)
// turns the flag into a filter, and it filters INSIDE the hour: a hidden hour
// does not count toward HoursSeen either.
//
// Chaos is the unit every absolute number is expressed in, and the rate is per
// HOUR. divineRateOf reads an hour's own divine/chaos market VWAP, so a play is
// valued at the rate of the hour it was observed in — pricing an old hour at
// today's rate is the same class of mistake as pricing a leg at another hour's
// low. Chaos-quoted legs convert at 1, divine-quoted legs at that hour's rate,
// and an hour with no divine/chaos trade simply does not clear its divine-quoted
// candidates. A play whose quote is neither chaos nor divine — scarab/scarab,
// card/scarab and every other cross-item market — is dropped outright, because
// gates denominated in chaos cannot judge a payout measured in scarabs.
// Result.DivineChaosRate is the same helper run once more on the window's NEWEST
// hour alone. Because a served play must have cleared in that hour, it is also
// the rate every play in the list was valued at; the per-hour rate governs the
// OLDER hours, which contribute only HoursSeen. It is 0 when the newest hour had
// no divine/chaos trade, in which case no divine-quoted play can be in the list
// either. The warning fires only when NO hour in the window carried that market,
// once per HORIZON and so twice per Service recompute.
//
// The gates, in the order the code applies them, with DefaultConfig's values.
// Per leg per hour, in gatedLeg: at least MinVolumePerHour (10) units of the
// leg's item traded, and stock on both sides of the market. Per candidate per
// hour, in evaluate: a quote that cannot be valued in chaos in this hour, then
// HideSuspect, then RoiPct >= MinEdge (0.001) — on the UNDERCUT return, so a tick
// that eats the spread fails here — Turnover >= MinTurnoverChaos (0), Tick <=
// MaxTick (1), RoiPct >= MinEdgeTickRatio * Tick (0 steps), and Roi >=
// MinROIChaos (0 chaos per exchanged unit). Then across the window, in
// BestPlays: HoursSeen >= MinHoursSeen (2 on the base config, capped at the
// hours actually present so a short window still returns plays, and overridden
// per horizon), and the newest-hour rule above.
//
// Four of those five are deliberately at values nothing can fail. Since POE-191
// the server serves everything sane and the QUALITY judgement is the client's:
// the desktop carries the four levels this package used to enforce (10,000
// chaos/hour of turnover, a tick no coarser than 10%, an edge at least 5 steps
// wide, 3 chaos per exchanged unit) as user-editable knobs whose defaults are
// exactly those numbers, so the out-of-the-box view is unchanged while a reader
// who wants cheap fragments or 1-hop triangles can have them without a redeploy.
// What stays server-side is what is not a matter of taste: liveness
// (MinVolumePerHour), persistence (MinHoursSeen), positivity (MinEdge, the
// sanity floor), the suspect flag, and MaxPlays (500) as a payload guard rather
// than a gate. A losing round trip cannot be served even by setting MinEdge
// negative: withDefaults clamps MinEdgeTickRatio and MinROIChaos to at least 0,
// and Roi >= 0 is the sign of RoiPct because Investment is positive.
//
// Those four LEVELS come from 30,534 priced Allflame market-hours: price
// quantization is the strongest single predictor of an apparent spread
// (corr(ln edge, ln tick) = +0.42, p50 tick 14.3%), which is what MaxTick and
// MinEdgeTickRatio answer, and chaos-denominated flow predicts a real edge where
// unit volume does not (−0.30 against +0.06; p50 robust edge 242% under 100
// chaos/hour and 18% over 100k), which is what MinTurnoverChaos answers. They
// were calibrated against that measurement's robust statistic — the p50 of a
// market's per-hour edges, under which the set took 908 markets to 135 — while
// the engine gates one hour at a time and counts the hours that cleared, so
// 908→135 is a calibration of the levels and not a prediction of how many plays
// come back; the served ranking was confirmed by a live check against the
// running stack. Enforced server-side on 2026-08-20 they took a newest hour of
// 1368 markets (881 clearing both-side liveness) to 79 served plays, which is
// the count POE-191 opened up.
//
// Ranking is clean before suspect, then RoiPct desc, then Turnover desc, then
// direct before 1-hop (one execution risk instead of three), then Key ascending,
// truncated to MaxPlays (500). It is a stable sort over a key-sorted list, so
// identical rows produce identical output whatever order they arrived in.
//
// One caveat governs every percentage. The feed publishes each hour's realized
// LOW and HIGH, not a book: both are trades that happened somewhere inside the
// same hour, and nothing says the two sides were takeable at the same instant, so
// a play's percentage is that hour's OPTIMISTIC reading of that hour. What bounds
// the fiction is the undercut the percentages are charged, Fair standing beside
// every price, Suspect when an extreme is too far from Fair to be repeatable, and
// — at their defaults — the client's tick knobs (the server's own tick gates are
// off since POE-191). Nothing is synthesized across hours: every number belongs to
// the single hour LastHour names.
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
// things, which is the point — a recent play has cleared in at least four hours,
// a day play in at least eighteen. A horizon overlays Config.WindowHours and
// Config.MinHoursSeen and nothing else; BestPlays ignores Config.Horizons itself.
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
// The feed sends metadata ids and nothing else — no display name, no icon, no
// category — so all three are carried in the binary. itemdata/items.json is a
// committed asset, `{"<metadata id>": {"category": <sidebar category>, "icon":
// <poewiki URL> | null, "name": ...}}` sorted by id (keys too), embedded by items.go
// and parsed once at package init. A malformed asset panics there: it is a
// build artifact, so the defect is in the committed file and is the same in
// every process.
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
// Item.Category is the in-game Currency Exchange sidebar category, resolved
// offline by the generator because the metadata bucket is not that taxonomy
// (oils, catalysts, omens, tattoos and runegrafts all sit under
// Metadata/Items/Currency/). Categories returns the sidebar's sixteen in
// sidebar order — a fixed list, not the distinct values the asset carries, so a
// category the exchange happens not to trade this league is still a filter row
// rather than one that appears on its own later.
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
// prints coverage per metadata bucket and per sidebar category. Those URLs are
// committable because MediaWiki derives the /images/<h>/<hh>/ path from the MD5
// of the File NAME, so a re-upload under the same name keeps the URL. The whole
// run is about thirty requests. The sidebar category has no upstream: an ordered
// prefix-plus-substring rule table in the script maps every id onto one of the
// sixteen. A rule naming a category the sidebar does not have fails the run
// before the first request; a name-coverage shortfall, a cache-filename
// collision, or an id no rule matches refuses the write after the fetch.
// Its output is deterministic, so an unchanged upstream re-runs to a
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
// window's), divineChaosRate (the NEWEST hour's divine/chaos VWAP and the rate
// every play in the list was valued at, 0 when that hour did not trade the
// market, in which case no divine-quoted play is in the list), count and plays.
// Each play carries key, mode, legs, roiPct, edge (its deprecated alias),
// roiPctRaw, roi, investment, turnover, tick, depth, suspect, hoursSeen and
// lastHour; each leg action, item, quote, price, fair, fairOk, tick, volume,
// stock and suspect. Every served body also carries categories, the sidebar's
// sixteen in sidebar order — the whole taxonomy, independent of the plays in
// this one, so the client's filter is not a function of the ranking.
// A COLD cache answers 200 with an empty plays list, warm: false and
// lastUpdated: null rather than an error or a database fallback — the recompute
// is the only reader, so a fallback query would just repeat it. The handler
// never touches the database.
//
// Each leg gains six transport-only fields; the engine itself never carries
// display data. itemName and quoteName come from DisplayName (the asset, with
// Humanize as the fallback for an id it does not cover, noted once through
// UnknownItems), itemIcon and quoteIcon from IconPath — API-relative paths
// into this server's icon route, or null for an item with no artwork, which the
// client renders without one — and itemCategory and quoteCategory from
// Item.Category, plain strings where "" (an id the asset does not cover) is
// what a filter treats as unfiltered, so it needs no absent-versus-empty
// distinction. Both sides carry one because a filter applies to whichever side
// the reader is shopping for. The icons are served by
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
// EXCHANGE_MAX_PLAYS and the four quality knobs EXCHANGE_MIN_TURNOVER_CHAOS,
// EXCHANGE_MAX_TICK, EXCHANGE_MIN_EDGE_TICK_RATIO and EXCHANGE_MIN_ROI_CHAOS.
// Those four default to off since POE-191 and each accepts a positive value
// only, so a deploy can re-arm one server-side (raising the floor under every
// client) but cannot loosen what is already open.
// The junk flag has three of its own: EXCHANGE_SUSPECT_LOW_BAND and
// EXCHANGE_SUSPECT_HIGH_BAND move the bands (both positive fractions of an
// hour's VWAP; nothing enforces low < 1 < high, and inverting them flags every
// leg rather than none, which is loud enough to notice), and
// EXCHANGE_HIDE_SUSPECT is a bool — parsed by strconv.ParseBool, so "1", "t",
// "TRUE" and "false" all read — that drops flagged plays instead of ranking them
// last. The window knobs are per horizon: EXCHANGE_RECENT_WINDOW_HOURS /
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
