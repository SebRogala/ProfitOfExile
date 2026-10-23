# Server trade gate hardening review — 2026-10-23

Status: historical implementation findings for POE-297. Canonical for this
investigation's evidence and verification limits; the tracker remains canonical
for acceptance criteria. Current behavior: [trade lifecycle](../TRADE-LIFECYCLE.md).

## Incident 5 — Single-request rejection paused unrelated lookups · severity: high

What: **Observed:** the round-1 implementation applied the shared-IP fallback to
every single 4xx, allowing one invalid gem query to pause unrelated lookups for
up to 30 minutes. Its per-key retry delay remained five minutes.

Root cause:

1. **Observed:** fix-round-1 wording equated contributing to GGG's invalid-request
   count with already being restricted. The limiter and test implemented that
   broad condition; fix round 2 explicitly supersedes it.
2. **Inferred:** a persistently bad key could outlast its five-minute delay during
   a longer global pause, time out in the queue without renewed scheduling
   metadata, and lead selection again after resume. This was the reviewer's
   compound-stall scenario, not an observed production trace.

Caught by: the orchestrator's second review, after gofmt, two race runs, all Go
packages and 167/167 integration checks had passed on the preceding state.

Prevention applied: immediate headerless fallback now covers only 401/403/429;
ordinary single 4xx failures stay key-local. A shared rolling counter trips on
five headerless non-429 4xx within ten minutes. Research publishes no K/W, so
these named constants are conservative local choices. Every parsed restriction
still applies globally. The limiter tests pin status distinctions, threshold,
sliding-window expiry and excluded response classes.

Request-scoped 4xx keys now wait six hours, allowing other stale keys to advance
without repeatedly asking the same rejected query. Transient failures retain
five minutes. Queue timeout overlapping a global restriction records a delay
until five minutes after restriction expiry or timeout processing, whichever is
later, preserving any longer key delay. Read-only review caught that checking
only active restrictions missed delayed callbacks; retained deadline evidence
now covers callbacks and dequeue processing after restriction expiry.
`TestGate_FailedLookupAllowsNextStaleKeyRefresh`,
`TestGate_QueueTimeoutDuringRestrictionDefersKeyPastExpiry`,
`TestGate_QueueTimeoutWithoutRestrictionKeepsStaleEligibility` and
`TestGate_BadKeyIsNotFirstPickAfterFloodPause` pin these outcomes. This replaces
Incident 3's broad global-status policy; no additional prevention is proposed.

## Incident 6 — Unchecked lookup inputs could trip the flood breaker · severity: high

What: **Observed:** the HTTP lookup handler accepted arbitrary bounded gem
strings. Five invalid-name lookups could contribute to the shared flood breaker;
round 3 requests rejecting unknown names before gate submission.

Root cause: **Observed:** transport validation checked only field presence and
length. The candidate `lab.Cache.GemDictionary` is OCR-filtered, so using it as a
complete denylist would also reject known support and Heist gems.

Caught by: the orchestrator's third review after gofmt, three race runs and all
Go packages passed on the preceding state.

Prevention applied: the handler uses exact OCR dictionary hits as a positive
cache and `trade.Repository.KnownGem` for raw shared-name/scoped-market membership
on misses. HTTP 400 JSON errors reject unknown names and malformed numeric
variants before `Submit`, including forced requests. Cold/empty/missing data
permits lookup as directed; actual membership-query failures return 503 without
submitting. `TestTradeLookup_UnknownGemNeverReachesGate`,
`TestTradeLookup_KnownNamesReachGate`, `TestTradeLookup_InvalidVariantNeverReachesGate`,
`TestTradeLookup_ColdDictionaryAllowsLookup` and the warmup/query-failure tests pin
those boundaries through a fake submitter. Repository integration tests pin raw
support/Heist/corrupted membership and league isolation. No additional prevention
is proposed for this handler change; the other entry points remain report-only
under the round-3 scope.

## Incident 7 — Contributions could bypass lookup validation · severity: high

What: **Observed:** `/api/trade/submit` still accepted arbitrary bounded names
and client-chosen old timestamps. Untiered ticks selected stale cached names
containing ` of `; repeated contributions could also clear per-key failure parks.
This path was reported but explicitly left unchanged by round 3.

Root cause: **Observed:** the lookup guard did not run before contribution cache
writes or persistence. Contribution `fetchedAt` was not bounded by receipt time.

Caught by: the orchestrator's fourth review after round-3 race, all-package,
169/169 integration and gofmt gates passed.

Prevention applied: both routes now invoke one `validateTradeInput` helper and
receive the same dictionary/checker from server wiring. Warm unknown names and
malformed variants receive JSON 400 before either write. Cold/empty dictionaries
retain the directed startup allowance. Contributions with missing, future or
at-least-five-minute-old timestamps are stamped with receipt time before both
cache Set and persistence enqueue; recent timestamps remain unchanged. Five
minutes matches the current tick stale window. Resetting to receipt prevents a
clamped ancient entry from immediately becoming the oldest candidate.

`TestTradeSubmit_UnknownNameNeverReachesCacheOrPersistence`,
`TestTradeSubmit_KnownNameReachesCacheAndPersistence`,
`TestTradeSubmit_InvalidVariantNeverReachesCacheOrPersistence`,
`TestTradeSubmit_ColdDictionaryAllowsContribution`,
`TestTradeSubmit_TimestampClamping` and
`TestTradeSubmit_AncientTimestampDoesNotLeadStalePicker` pin those outcomes.
`TestTradeLookup_CachedUnknownNameStillRejected` pins non-forced cache-hit
validation. The server wiring edit was expressly authorized by the orchestrator
because the round-4 path list initially omitted it. No further prevention is
proposed; accepted limits remain below.

## Incident 8 — Cold-window junk remained refreshable after warm-up · severity: medium

What: **Observed:** cold-dictionary contributions could be cached and persisted,
then restored by `Warm(LatestLookups(...))`. The live untiered picker filtered
variant and ` of ` shape without checking whether the restored name was known.

Root cause: **Observed:** input validation covered new requests once warm,
but cache retention and startup warming could reintroduce unchecked old names.

Caught by: the orchestrator's fifth review after round-4 race, all Go packages,
169/169 integration and gofmt gates passed.

Prevention applied: the live `HandleTradeTick` picker uses `KnownTradeGem`, the
same helper `validateTradeInput` calls, with `Repository.KnownGem` wired from
`cmd/server`. Each rejected candidate is excluded for the current tick only,
allowing the next known stale key to be selected without deleting or restamping
entries or rows. Checks run outside the cache lock; query errors abandon the
tick. Cold/empty dictionaries retain the requested skip-nothing policy.
`Refresher` has no production constructor call and was left unchanged.

`TestHandleTradeTick_WarmDictionarySkipsRestoredUnknownKey` pins the restored
oldest junk/next-known selection, OCR and raw-membership paths, and retention.
`TestHandleTradeTick_ColdDictionaryKeepsOldestUnknownKey` pins startup behavior;
unknown-only and query-failure tests pin the no-send paths. No additional
prevention is proposed.

## Incident 1 — Serial limiter waits delayed queued lookups · severity: medium

What: the pre-change gate waited for max(search, fetch) before searching and
slept that entire estimate. Queue age was checked only before sleeping. A
successful response was header-synced and then locally recorded, potentially
counting the just-completed request twice. Error headers never reached the
limiter. These observations are from the branch's pre-change gate and limiter.

Root cause:

1. **Observed:** a single gate worker held later requests behind these waits;
   fetch capacity could delay even a search returning no IDs. The queue deadline
   was not rechecked after sleeping, so an expired request could still send.
2. **Observed:** header sync injected server-reported usage before Record added
   another local hit. This could consume the ceiling earlier than one slot per
   attempted request would.
3. **Inferred:** these mechanisms explain the supplied production timeout report
   (226 queue timeouts, approximately 17%, without search failures). Production
   traces were not replayed, so the contribution of each mechanism and the
   exact percentage are not established by this change. Legitimate budget
   exhaustion and queue load can still cause queue timeouts.

Caught by: the owner's production report, followed by source tracing.

Prevention applied: endpoint-specific bounded rechecks, Record before sending,
headers observed on errors, global restriction expiry, and independent queue
caller deadlines in `internal/trade/gate.go` and `ratelimiter.go`. Regression
specifications live in `gate_hardening_test.go` and `ratelimiter_test.go`.
No additional prevention is proposed.

## Incident 2 — Expired dedup caller retained the completion event · severity: low, process

What: while adding independent deadlines, completion still published to the
original request ID even if that caller had expired and another waiter survived.
This intermediate change remained uncommitted.

Root cause: **Observed:** the channel fan-out used live waiters, while Mercure
publication used the original queue item's request. Independent expiry made
those identities diverge.

Caught by: a read-only concurrency/test review before handoff.

Prevention applied: ready/error publication now uses the same live waiter set
as channel delivery; waiting events also fan out. The event-ID assertion in
`TestGate_LaterDedupWaiterSurvivesOlderDeadline` pins the outcome. Empty-flight
reservation is pinned by
`TestGate_ResubmitBeforeExpiredQueueItemConsumedUsesReservedFlight`.
No additional prevention is proposed.

## Incident 3 — Failed keys were immediately re-picked · severity: medium

What: **Observed:** upstream error handling retained the previous successful
`FetchedAt` with no separate retry delay. The stale picker therefore kept
selecting the same oldest failed key; headerless non-429 rejections supplied no
global pause. The research document says every HTTP 4xx contributes to the
invalid-request threshold.

Root cause: **Observed:** the implementation separated empty success from error
but recorded no scheduling metadata for error; the limiter recognised only the
429 sentinel when restriction headers were absent.

Caught by: the orchestrator's reviewer (F1); F6 also identified missing tests for
fallback window/penalty raising and the 30-minute cap.

Prevention applied: process-local per-cache-key five-minute failure delays,
independent of successful quote/check timestamps, and typed HTTP status errors
that initially applied the global bounded fallback to every status 400–499
(superseded by Incident 5). Pinned at that stage by
`TestGate_FailedLookupAllowsNextStaleKeyRefresh`, cache retry-boundary tests,
`TestObserveResponse_Headerless4xxBlocksBothPools` and
`TestObserveResponse_Headerless403FallbackExpires`. Separate
`TestObserveResponse_FallbackUsesLongerTierWindow`,
`TestObserveResponse_FallbackUsesLongerTierPenalty` and
`TestObserveResponse_FallbackCapsAtThirtyMinutes` pin each fallback branch.
The persistence provenance test now runs the persistence task synchronously
through an injected launcher and checks its channel without a wall-clock timer.
No additional prevention is proposed.

## Incident 4 — Rebased test assumed cache input identity · severity: low, process

What: the rebased failure-delay test failed in search400, fetch403 and search500.

Root cause: **Observed:** POE-302's `TradeCache.Set` stores a derived copy with
`MedianTop3`, while the test required the cached pointer to equal its input.
The supplied failure output retained the original quote and timestamp and
showed the derived median of 42. `RecordFailure` changes only retry metadata;
no production failure-delay regression was found in this path.

Caught by: the orchestrator's post-rebase focused test run.

Prevention applied: `TestGate_FailedLookupAllowsNextStaleKeyRefresh` now checks
quote identity fields, both medians, price, listing content and the independent
original timestamp by value. POE-302 production behavior is unchanged. Both
flagged test files were formatted by hand; runtime and gofmt verification remain
with the orchestrator. No additional prevention is proposed.

## Known residuals

- LOW: the clamp stamps a genuinely old contribution as receipt time; the owner reports desktop submissions arrive within seconds.
- LOW: once validation is warm, new-league gems absent from `gem_colors` and this league's `gem_snapshots` are rejected on submit.
- LOW: dictionary-query 503 responses have no `Retry-After` header.

- Names in `gem_colors`/`gem_snapshots` that GGG rejects can still feed the flood breaker.
- Each warm OCR-dictionary miss with a membership checker available costs one DB probe per request.
- The cold-dictionary allowance lasts until the first v2 run successfully populates it; failed seed reads prolong the window.

- The flood breaker uses penalty/window-derived fallback length, capped at 30 minutes; it is not a fixed short pause.
- A query-builder bug that earns request-scoped 4xx can park affected keys for six hours.
- Cold/empty/missing lookup dictionaries or missing name checkers permit names through; the limiter/breaker remains the fallback until validation is available.

- F2: fetch waiting after successful search has no deadline; a dedup joiner on a running lookup gets no queue timer.
- F3: explicit Retry-After/state restriction durations remain uncapped; queued callers see only a queue timeout, not the restriction deadline.
- F4: zero-listing rows retain numeric zero price fields rather than NULL; current readers guard on listing count.
- F5: high-priority resubmission can attach to an expired low-priority flight still in the low queue; the initial wait event covers only callers present when waiting begins.

## What worked (the net held)

Source tracing found that zero-listing cache stamping and consumer guards were
already implemented, avoiding duplicate production changes. Persisted reload
and source compatibility now have integration regressions. Read-only review
caught event attribution before handoff. The orchestrator reviewer caught missing
failure scheduling and fallback test gaps despite passing initial runtime gates.
The post-rebase focused test run caught the obsolete cache pointer assertion.
The second review caught disproportionate global pausing and the inferred delay
interaction despite passing runtime gates. The third review found the public
input route to the breaker; source tracing prevented an OCR-only allowlist from
rejecting support gems. The fourth review followed the reported contribution-to-tick
path through cache writes and persistence and closed the new-input bypass.
The fifth review followed retained cold-window rows through restart warming and
added the missing refresh-selection guard. Desktop code remains untouched.

## Upstream fixes (not mine)

None identified. ADR-027 was absent from this branch but was supplied on `dev`
by the orchestrator and read with `git show`; its cheapest-three amendment is
binding on price resolution, outside this gate-hardening change.

## Carry-forward checklist

1. Run focused trade tests with `go test -race` in the orchestrator environment.
2. Run repository integration tests through `scripts/integration-test.sh`.
3. Apply test-author mutation checks and restore each mutation against its saved pre-mutation content.
4. Run gofmt and review its diff before committing.

Verification: the round-5 brief reports race, all Go packages, 169/169 integration
tests and gofmt passed on the preceding state. Those checks predate the round-5
refresh-selection guard. Locally,
`git diff --check` passes. Go, gofmt, Docker, new race/integration runs and applied
mutation checks remain unavailable or reserved for the orchestrator. New tests
are authored, not runtime-verified.

Disposition: all applied prevention lives in the code and named regression tests;
this findings log records the evidence, review catch and pending verification.
