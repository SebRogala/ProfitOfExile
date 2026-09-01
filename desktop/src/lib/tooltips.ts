/**
 * Tooltip text content for all signals, windows, advanced signals, and metrics.
 * Sourced from FRONTEND-DESIGN.md "Tooltips — Detailed Signal Descriptions".
 */

export const SIGNAL_TOOLTIPS: Record<string, string> = {
	STABLE:
		'Price and listings are steady. Safe to farm \u2014 predictable returns.',
	UNCERTAIN:
		'Market movement doesn\u2019t fit a clear pattern. Check price and listing trends manually before deciding.',
	HERD:
		'Price AND listings both rising \u2014 multiple farmers flooding the market. Sell now if you have stock. Don\u2019t start farming, you\u2019re late.',
	DEMAND:
		'Listings draining while price holds \u2014 buyers are absorbing supply. Good time to sell at market price. Good farming target.',
	DUMPING:
		'Price dropping while listings rise \u2014 sellers undercutting each other. Avoid farming this gem.',
	RECOVERY:
		'Price drifting down in a thin market while supply dries up \u2014 potential bottom forming. Watch for price stabilization.',
	CAUTION:
		'Short-term volatility detected \u2014 price is swinging. Check recent price history before committing. Informational only, does not affect rankings.',
};

export const WINDOW_TOOLTIPS: Record<string, string> = {
	CLOSED:
		'No farming opportunity detected. Base gems available but no special conditions.',
	BREWING:
		'Opportunity forming! Price rising + trans listings falling + bases still available. Window may open in ~2 hours. Start planning your lab run. Triggered: price velocity > 0, listing velocity < 0, bases > 10',
	OPENING:
		'Base gems starting to drain. Window score is moderate. Prepare to act soon. Triggered: window score \u2265 50, base velocity < 0',
	OPEN: "Farm NOW! High ROI, low trans listings, bases draining fast. This window lasts 1-2 hours typically. Triggered: window score \u2265 70, base velocity < -2",
	CLOSING:
		"Herd arriving \u2014 other farmers' transfigured gems hitting the market. Sell immediately if you have stock. Triggered: trans listing velocity > 3",
	EXHAUSTED:
		'No base gems available on market. Unfarmable until bases reappear. Triggered: base listings \u2264 2',
};

export const ADVANCED_TOOLTIPS: Record<string, string> = {
	COMEBACK:
		"Was in the top gems previously, crashed, now showing recovery. Lower herd risk since it's no longer on poe.ninja's front page. Good for experienced farmers. Triggered: hist position < 30%, price rising, listings dropping",
	POTENTIAL:
		'Rising ROI that hasn\'t been widely noticed yet. Low competition, moderate price, rising trend. Best opportunity for experienced players who want low-herd-risk plays. Triggered: price 30-200c, < 40 listings, price rising, below historical midpoint',
	PRICE_MANIPULATION:
		'Suspicious pricing. Very few listings at high price with no movement. Likely someone trying to set a fake price floor. Avoid. Triggered: \u2264 3 listings, price > 200c, no velocity, high CV',
	CASCADE:
		'Thin-market extreme movement. Two possible causes: (1) Cascade \u2014 someone bought out listings, relisted high, undercutters driving price down. (2) Streamer effect \u2014 new build released, genuine demand spike. Either way this is RISKY \u2014 the system cannot distinguish the cause. Use your game knowledge to decide.',
	BREAKOUT:
		'Price breaking above historical range with rising listings. Genuine demand increase, not manipulation. Strong buy/farm signal. Triggered: price > 90th percentile, listings rising, positive velocity',
};

export const METRIC_TOOLTIPS: Record<string, string> = {
	ROI: 'Absolute profit in chaos orbs. Transfigured gem price minus base gem price. Higher = more profit per transfigure.',
	'ROI%':
		"Return on investment as percentage. ROI divided by base price \u00d7 100. Better for comparing across price tiers. A 20c gem with 200% ROI is better for small budgets than a 200c gem with 50% ROI.",
	CV: "Coefficient of Variation \u2014 how predictable the price is. Lower = more stable. Under 25% is safe, 25-50% is moderate, over 100% is a trap. Calculated from price standard deviation over 7 days.",
	EV: 'Expected Value from using Font of Divine Skill. Probability of hitting a profitable gem \u00d7 average winner price. Higher EV = better font usage.',
	pWin: 'Probability of getting at least one winner when the font picks 3 random gems from the color pool. Uses hypergeometric distribution. Higher = better odds.',
	Pool: 'Number of unique transfigured gems of this color. Smaller pool = better odds of hitting a specific winner. RED typically has smallest pool.',
	Liq: 'Base gem liquidity relative to market average. HIGH (\u226580% of avg) = herd gets absorbed, safe. MED (30-80%) = windows open and close. LOW (<30%) = bases drain instantly, short windows. Auto-adjusts for weekend/weekday and league phase.',
	'\u039412h': 'Change over the last 12 hours. Shows recent momentum. \u2191 = increasing, \u2193 = decreasing.',
};

/**
 * The Currency Exchange table's column and control tooltips (POE-186).
 *
 * Separate from `METRIC_TOOLTIPS` on purpose: the two surfaces share the words
 * "ROI" and "ROI%" and mean different things by them. A gem's ROI% is
 * percentage points off a base-gem price; an exchange play's is a fraction of
 * what one round trip costs, net of the undercut each of its trades pays.
 * Pointing a gem tooltip at this table would word the number wrongly by a
 * factor of a hundred.
 *
 * Every PRICE-shaped figure these describe is the LAST SETTLED feed hour
 * (POE-188), not the live book — which is why each entry ends where it does:
 * with what to do about that, rather than with the number as if it were a
 * quote. Exp. ROI and Hours are the two exceptions and both say so in their own
 * words: they read across the window on purpose (ADR-016), which is the whole
 * reason they are worth more than the hour beside them.
 *
 * Written for a reader who plays the game, not one who trades for a living
 * (POE-191): a play is a round trip made of TRADES or STEPS, never of "legs".
 * The word survives in the code, where it names a wire field, and nowhere the
 * reader can see it.
 */
export const EXCHANGE_TOOLTIPS: Record<string, string> = {
	ROI: 'Absolute profit in chaos orbs at ONE POSTING of the market you enter on — the order you can actually place, once, whatever currency that market prices in. That is the size the route above is priced for, and it is what you would have made if BOTH of the hour’s extreme prices had been there for you on every exchange, net of one price step of undercut on each of the play’s trades. That is the hour’s best case, not what the play pays: Exp. ROI beside it is the measured answer, and it is what the Scale column and the ranking use. The route ends on the same figure: its LAST STEP totals what the hour’s best case would have paid, which is the Spend plus this column — except where a step has no total to print at all, which its own hover says. A row whose entry market posted no quantity pair counts ONE item instead, the same SIZE the route’s two ends fall back to, so the row is never half one size and half the other. The size is all they share: the route’s Get end is the Spend plus Exp. ROI on every row, never the Spend plus this column, which is why a measured loser can end below what it cost. Both identities are in chaos, and a divine-entry route prints them at the divine rate. Every figure here is the last settled hour, so verify the route in game before committing.',
	'ROI%':
		'Return on investment as a percentage — ROI divided by what it cost. Scale-free in the literal sense: whatever size the row is priced at multiplies the ROI and Investment columns by the same count, so this is the same percentage per exchange as it is per posting or per run, and it compares plays across price tiers either way. NET is net of one price step of undercut on every one of the play’s trades, the best case an order that actually gets taken could have had in that hour. RAW is the same round trip at the hour’s raw extremes — never below NET, and the gap between them is what those steps cost. Neither is what the table is ranked by any more: Exp. ROI is. NET is still what your Gates row judges.',
	'Exp. ROI':
		'What posting this play’s orders would have paid, in chaos at ONE POSTING of the market you enter on — the order you can actually place, once, whatever currency that market prices in — and ONE item where that market posted no quantity pair at all. It is the same chaos the route’s “keep ≈” line carries, or its “lose ≈” line when the measurement came out negative. The route’s Get end is the Spend plus this figure in BOTH cases, the negative one included, so a play that measured a loss ends the row below what it cost. That identity is in chaos, and a divine-entry route prints it at the divine rate. Every hour of the last day is replayed: your buy goes up one step above the hour’s cheapest buy price and chases the market up if it does not fill, your sell sits one step under the dearest sell price and waits up to three hours for someone to take it, and whatever never sold is dumped at the last hour of that wait, halfway between its average price and one step under its dearest. The mean of those outcomes is what ONE exchange pays, and this figure is that mean at the row’s size — so it is what one order of that market is expected to make, which on a cheap market is a fraction of a chaos and prints here as a bare 0, this column counting whole orbs — the row is still drawn as a gain, because the measurement is positive even where the rounding is not — while the Scale column beside it says how many of those orders clear about 100 chaos and what the whole run would then gain. The Exp. ROI sort orders the table by this column’s own posting-priced figure and by nothing else; the order the server sent survives only where two rows tie on it. It CAN be negative, and then the play is simply expected to lose: it is still shown and simply ranks below the ones that measured well, with a dash in the Scale column because no number of repeats turns a loss into 100 chaos. n is how many hours were replayed; LOW COVERAGE means too few of them to trust the mean, not that the play is bad. Measured across 960 top-20 play-hours the ROI column overstates this by four to eight times, and that measurement is of DIRECT flips: a 1-hop route is replayed the same way but nothing has checked that a triangle behaves like a flip.',
	Investment:
		'Chaos this row ties up at ONE POSTING of the market you enter on — and ONE item where that market posted no quantity pair at all — priced at the undercut entry. It is what a single order costs, on every row and in every entry currency; the whole run’s cost lives in the Scale column’s “N c in” sub-line instead. The filter bar’s Run cost bounds are compared against the RUN wherever there is one, because a bankroll ceiling is a run-sized question — so they read that sub-line’s figure and not this column, and on a row with no run at all they fall back to what one exchange costs. Switch those bounds to divine for the large ones — converted at the divine/chaos rate from the same feed hour.',
	Gold: '(column hidden until computable) The in-game currency exchange charges gold per trade. Nothing here is net of it yet, and a reserved column of dashes promised a number the page could not give, so the column is gone until the per-trade cost is known and ROI can be shown net of gold.',
	Route:
		'The play as five slots: what you spend, the two or three trades, what you get back. The SIZE is ONE POSTING of the market you enter on — the order you can actually place, once: “buy 1 for ≈ 24c”, “buy 4 for ≈ 1c” where the market posts four at a time, “buy 16 for ≈ 1.01 div” where it posts sixteen for a divine. The currency you enter with does not change that, and neither does what the play measured. Where that market posted no quantity pair at all the row counts ONE item, which is the same size the route’s two ends fall back to, so the row is never half one size and half the other. The Scale column shows the worthwhile run wherever there is one, so the row still tells you how many of these orders clear about 100 chaos and what that would tie up; where there is none it shows a dash and the run is nowhere on the row. Spend is what the buys cost at the undercut entry, in the currency you actually pay with: a route entered in divine reads in divine, with the chaos equivalent under it. Get is that same figure back plus the expected profit at the same size, and the line under it says that profit in chaos. Each step names the ITEM it acts on and reads as a TOTAL carrying a ≈ — “buy 12 for ≈ 3c”, “sell 16 for ≈ 222c” — never a per-unit price like “0.25 div each”, which the exchange has no field for and the game will not let you post. The ≈ is on the PRICE and not the quantity: the count is exact, the price beside it is the order that actually fills, one price step tighter than the hour’s posted extreme. Hover a step to see what that market printed. The currency is on every step — c for chaos, div for divine — so a route that trades against divine shows you where; a step quoted in neither prints its amount bare rather than label it with a unit word that would be wrong. Where a market only posts in lots the row’s count does not divide, that hover says how many whole orders that is and what is left over; where the market you enter on posts more at a time than the whole worthwhile run needs, it says that too — one order is the smallest trade it will take, so the row counts it whole. All of that holds except where a step has no total to print at all: such a line falls back to what its own market posts — one order of it, or the bare per-unit rate when that market posted no pair — and its own hover says so. The last step of a 1-hop converts the proceeds of the sale into the end of the chain. That last total is what the hour’s best case would have paid — the Spend plus the ROI column — while Get is what the last day measured, the Spend plus the Exp. ROI column; the gap between the two is the gap between those two columns. Both identities are in chaos, and a divine-entry route prints them at the divine rate.',
	Trend:
		'Reserved for the fair-price trend across recent hours — whether the market this play trades against is drifting up or down. No per-play fair history is published yet, so nothing is derived and the cell shows a dash rather than a direction the data cannot support.',
	Depth:
		'Units per hour the thinnest of the play’s trades saw — the hourly ceiling. This is the whole market’s volume, not your share, and a direct play buys and sells the same item on one market, so its real ceiling is lower still. It is what the Scale column divides the worthwhile flip count by to answer how long the market needs.',
	Scale:
		'The flip count at which this play clears about 100 chaos of EXPECTED gain, what those flips tie up, and how long the market needs to absorb them. This column is the RUN and never the size the row displays: it is the ONLY place on the row the run appears, because the route and the money columns beside it count one posting of the market you enter on instead. It counts Exp. ROI and not ROI, so the size is the one you would actually have had to run: a play whose expectation is 3c an exchange reads ×34, one worth 150c reads ×1, and a play that is not expected to gain at all shows a dash — there is no number of repeats that turns a loss into 100c. The “N c in” figure is what the filter bar’s Run cost bounds are compared against — on every row that prints one, which is every row that has a run at all. A row showing the dash prints no “N c in”, and there the bound falls back to what ONE exchange costs. The hours are UNCONTESTED — they assume the WHOLE hourly volume on the play’s thinnest trade is yours and nobody else is competing for it — so read the wait as the floor on the time, not the time. Green is inside the hour, which is the only case where the ROI was computed against a book that will still be there.',
	Hours:
		'How many of the window’s hours this play held — the persistence count. An hour counts when the play was ALIVE in it (at least ten units of each traded item changed hands, with orders standing on both sides) and its return cleared the server’s +0.1% sanity floor. That is the whole test since POE-191: the quality bar moved to your Gates row, so these hours no longer ask a play to have been worth trading, only to have existed and paid. Expect the fraction to read higher than it used to. The server still needs a minimum before it serves a play at all — four of the recent window’s six hours, eighteen of the day window’s twenty-four — so the full count is a spread that stood all window and the minimum is one that only just persisted.',
	'Only / Hide':
		'Two layers. Category pills are coarse — the 16 buckets the in-game exchange lists down its own sidebar. Item chips are overrides: an item rule beats whatever its category says, and Hide beats Only when both apply. A play matches if either side of any of its trades hits a rule — what you buy or what you pay with. Both layers are remembered across restarts.',
	Mode: 'DIRECT buys and sells the same item on one market — market making, two trades. 1-HOP buys an item against one currency, sells it against another, then converts back — three trades, three chances to be beaten to the fill.',
	Suspect:
		'One of the play’s trades is priced outside its fair band: a buy below fair × 0.67, or a sell above fair × 1.5. The play is still served and sits wherever the column you sorted by puts it — the flag is a MARK on the row and never a demotion, because the extreme may be a real fill or a single stray order and only the book can say which. Read the row as a signal, not a quote — verify the route in game before committing.',
	'Depleted side':
		'One side of this trade’s book was empty in the hour — nobody was standing opposite you. It is both the danger and the opportunity: an order posted into an empty side may wait there alone, with nothing to match against until someone else arrives, and the edge is there at all BECAUSE nobody is competing for it. Neither reading is the right one on its own, which is why this is a mark and not a verdict — the play is served, ranked and priced exactly as any other. It is a fact about the book’s SHAPE and not about the price, so it is independent of the suspect mark: a one-sided book can quote a perfectly ordinary price, and a price far outside its fair band can have had a full book on both sides. The mark sits on the step whose book was one-sided, so you can see WHICH trade of the round trip had nobody facing it. Check that side of the book in game before posting.',
	'Low liquidity':
		'The newest hour printed no spread worth taking on this play: the round trip at that hour’s undercut prices returned less than the server’s floor. The ROI% beside it is that measured return and not a placeholder — it can be negative, and a market whose whole price step is 100% reads −100% there. The usual cause is a THIN HOUR, an hour so quiet that the little which traded all cleared at one ratio, which is not the same thing as a dead market: this was a reason to drop the play until 2026-08-22, and a card flipping at 70–92% in five of the window’s other six hours vanished from the table for one 2-item hour. So it is a mark now, never a hiding: Exp. ROI beside it still measures the whole day and still ranks the row, so a play whose quiet hour is an exception keeps its place and one that never had a spread sinks on its own. Read the entry prices on this row as its weakest part and check the book in game before posting.',
	'Step not used':
		'The dashed tile is the convert step a DIRECT play does not take — it buys and sells the same item on one market, so there is nothing to turn back into the currency it started in. The slot is held open rather than closed up so the Get column never lands under the row above’s sell; when no play on screen converts at all, the column goes away entirely.',
	'Expected gain':
		'Green is what the row is EXPECTED to end with above what it cost — the Get amount on a play whose measured expectation is positive, and only that amount — the profit line under it stays grey either way. A play whose expectation measured NEGATIVE gets no green: its Get is still the Spend plus that measurement, so it prints below what the row cost and its line under it reads “lose ≈” instead. There is deliberately no red here — Exp. ROI beside it already owns the red for exactly that reading (Measured loss), and two reds on one row saying one thing is a worse signal than one. The absence of green is the signal.',
	'Measured loss':
		'Red on Exp. ROI is a READING, not an error: the simulation replayed the play across the last day and it came out negative. Such a play is still served and still sorted on that reading like every other row (ADR-016) — hiding it would be hiding the measurement.',
	'Low coverage':
		'A dimmed Exp. ROI with a “low” marker means too few of the last day’s hours could be replayed to trust the mean. The verdict on the number is unchanged; the confidence in it is what dropped.',
	'No reading':
		'A dash is a question this row has no answer to — a column that computes nothing yet, a play with no simulable hour, or a scale that cannot be derived. It is never a zero: printing 0 would claim the play was measured and came out flat.',
	'Full window':
		'A full green bar is a play that held EVERY hour of the window. The colour is the persistence verdict, not a progress indicator — a partial bar is grey however close to full it is.',
	'Data age':
		'Every figure on this page is the last SETTLED feed hour, not the live book. The feed publishes 40–60 minutes after an hour closes, so these prices can be up to about two hours behind what the exchange is showing you right now. Check the route in game before committing to it.',
	'Run cost':
		'What the worthwhile run ties up — the figure the Scale column’s “N c in” sub-line prints, not the cost of one exchange and not the cost of one posting. The bound reads that very number, so a ceiling you type here can never let through a row whose Scale column sits above it. The Investment column beside it answers a different question: every row displays one posting of the market you enter on, so that column is what a single order costs while this bound asks what the whole run would tie up — which is the question a bankroll asks. Set the min/max to see only what your bankroll covers; the divine toggle converts the bounds at the newest hour’s rate. A play with no run is judged on what one exchange costs.',
	Gates:
		'The quality bar — six floors and one ceiling. Four floors and the ceiling are what the SERVER used to apply to everybody before it sent anything (POE-191 handed them to you) and all five ship OFF: an empty box filters nothing, so the table shows everything the server served and the ranking does the judging. Each tooltip names the level worth typing if you want the old, stricter table back. The two floors the row OPENS with are the exceptions — Min item price at 0.5c and Min item price (div) at 0.4 div, the first two boxes on the row and the only filters armed out of the box, and they are there to keep entries too small to be worth flipping from crowding the ones that are. They are one line drawn in two currencies: the div floor only ever looks at plays you enter by spending divine, so a chaos-entry row is governed by the chaos floor alone. Clear leaves the whole row alone; Defaults empties every box, which is those five off and both item-price floors back on.',
	'Min item price':
		'The least ONE exchange may COST to enter, in chaos — the price of the thing you are flipping. It is deliberately per-exchange and the Investment column beside it is not: that column counts what the row DISPLAYS — one posting of the market you enter on — so unless that posting happens to be a single item, nothing on screen prints the number this floor is compared against. That stays deliberate because the 0.5 is a calibrated price of a THING: judging it against a posting would multiply the level you type by each row’s own count, so one number in the box would mean a different floor on every row and stop being a level at all. This is one of the two filters this app arms for you (Min item price (div) is the other): it ships at 0.5, which drops the bottom of the sub-chaos tier (Clear Oil and its neighbours). Not because those markets are fake — they are as real and as finely priced as any other — but because an entry under half a chaos pays too little per flip to be worth the repeats, so those rows crowd out the ones you would act on. The 0.5 is chosen to keep the fragment and oil tier: a flip entered at 0.5c or more is still on the table, and only the very bottom goes. Everything else here starts off, and the server sends everything it can stand behind. To disable it, type 0 — the table then shows the fractional stuff too. Blanking the box does NOT disable it — an empty box means whatever this build ships, which is the 0.5. Exactly 0.5c gets through; only cheaper is dropped. A 1-hop route is judged on the same number, the chaos it costs to enter. It judges every row, whatever currency you enter with, because that entry cost is always priced in chaos — on a divine-entry row the div floor beside it is the line that actually bites, since 0.4 div is worth far more than half a chaos.',
	'Min item price (div)':
		'The least ONE exchange may COST to enter, in DIVINE — the same question Min item price asks, in the currency you are actually spending, and it only ever looks at plays whose entry market quotes in divine. A chaos-entry row is never touched by this box however cheap it is; that row is the other floor’s business. It ships at 0.4 div and drops the hundreds-per-divine tier: an item you buy at three hundred to the divine is not a flip you can price, because every competitor sits inside one price step of you and the whole edge is an undercutting war. Items priced in real fractional divines start being worth the trip at about four tenths of an orb each, which is where the level is. This is one of the two filters this app arms for you (Min item price is the other, at 0.5c) and it is armed for the same reason: not because those markets are fake — a hundreds-per-divine market quotes as finely as any other — but because the size is too small to act on, so those rows crowd out the ones you would. WHICH FIGURE IT COMPARES: what one exchange costs to enter, read in divine — the chaos entry cost divided back by this hour’s divine rate, which is the rate the server priced it with in the first place. Like Min item price it is per-exchange while the columns count the size the row displays, so nothing prints it unless that size is a single item: on a divine-entry row whose market posts one at a time, the route’s Spend slot IS this number. The Investment column is not it in any case — that column is always in chaos. To disable it, type 0; the div side of the table then shows the fractional stuff too. Blanking the box does NOT disable it — an empty box means whatever this build ships, which is the 0.4. Exactly 0.4 div gets through; only cheaper is dropped. A 1-hop route is judged the same way, on what it costs to enter.',
	'Min profit':
		'The least chaos a play must gain on ONE exchange, at the hour’s best-case prices — the best-case number the ROI column is built from, before that column multiplies it up to the size the row displays, and never Exp. ROI. A gate asks whether the market is worth trading at all, never how far the play has to be repeated to pay, which is the Scale column’s answer; so like Min item price it reads one exchange while the columns read the size the row displays, and no column prints the figure it compares against — unless that size happens to be a single exchange, where the ROI column beside it IS this number. That is the same narrowing Min item price carries and it comes from the same place: a market that posts one at a time displays exactly one exchange. That is kept on purpose: the 3 below is the floor the SERVER applied per exchange, and re-pointing this gate at a posting would multiply it by each row’s own count — “3c profit” would mean 3c per order on a one-at-a-time market and twelve times that on a market that posts twelve, so the number in the box would stop being a level at all. Empty (or 0) is off, which is how the cheap plays stay visible: sacrifice fragments and the like gain a fraction of a chaos each and only add up on volume. Type 3 for the old server floor.',
	'Min turnover':
		'How much chaos had to change hands on the play’s market during the feed hour. Empty (or 0) is off, so the quiet corners of the exchange are on the table. Type 10000 for the old server floor — below that you are not joining a market so much as being one, and your own order sets the price. It is a blunt line: a real flip turning over 8,500c an hour fails it.',
	'Max price step':
		'The coarsest price step the play’s market may quote in, as a percent of the price. A market that only moves in 25% jumps cannot be undercut finely, so the entry you planned is not the entry you get. Empty (or 0) means no ceiling. Type 10 for the old server level.',
	'Edge vs step':
		'How many price steps wide the play’s return has to be. At 5 the return must be five times the market’s own step, which keeps a play whose entire edge is one step of rounding off the table — but it also takes nearly every 1-hop route with it, because the divine step alone eats them. Empty (or 0) is off, which is why they are on the table. Type 5 for the old server level.',
	'Min return':
		'The least return a play must show, as a percent of what one exchange costs — the same NET figure the ROI% column prints. Empty (or 0) is off, which shows everything above the server’s +0.1% sanity floor — it never serves a play that loses money or gains only float noise. Type 2 for the floor the server used to apply to everyone.',
	Counter:
		'What is left of the response, and what took the rest. Gates are counted apart from everything else because their controls sit behind a collapsed row — the split points at the row that would give those rows back. Five of the seven ship off, so anything this reads before you touch a knob is the two item-price floors dropping the sub-chaos and hundreds-per-divine tiers. Everything else is counted together: the category pills, the item chips, the run-cost bounds, and the search box.'
};

export const SELL_CONFIDENCE_TOOLTIPS: Record<string, string> = {
	SAFE: 'Liquid market, stable price \u2014 will sell near listed price',
	FAIR: 'Moderate risk \u2014 may need patience or small undercut',
	RISKY: 'Thin market or volatile \u2014 significant gap between listed and realizable price',
};

export const LIQUIDITY_TOOLTIPS: Record<string, string> = {
	HIGH: 'High liquidity \u2014 herd gets absorbed, safe to farm. Base listings \u226580% of market average.',
	MED: 'Medium liquidity \u2014 windows open and close. Base listings 30-80% of market average.',
	LOW: 'Low liquidity \u2014 bases drain instantly, short windows. Base listings <30% of market average.',
};
