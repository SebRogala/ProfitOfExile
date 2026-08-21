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
	ROI: 'Absolute profit in chaos orbs on ONE exchange — chaos in, chaos out — if BOTH of the hour’s extreme prices had been there for you, net of one price step of undercut on each of the play’s trades. That is the hour’s best case, not what the play pays: Exp. ROI beside it is the measured answer, and it is what the Scale column and the ranking use. Every figure here is the last settled hour, so verify the route in game before committing.',
	'ROI%':
		'Return on investment as a percentage — ROI divided by what one exchange costs. Scale-free, so it compares plays across price tiers. NET is net of one price step of undercut on every one of the play’s trades, the best case an order that actually gets taken could have had in that hour. RAW is the same round trip at the hour’s raw extremes — never below NET, and the gap between them is what those steps cost. Neither is what the table is ranked by any more: Exp. ROI is. NET is still what your Gates row judges.',
	'Exp. ROI':
		'What posting this play’s orders would have paid, in chaos per exchange — the ranking’s number. Every hour of the last day is replayed: your buy goes up one step above the hour’s cheapest buy price and chases the market up if it does not fill, your sell sits one step under the dearest sell price and waits up to three hours for someone to take it, and whatever never sold is dumped at the last hour of that wait, halfway between its average price and one step under its dearest. The mean of those outcomes is this figure. It CAN be negative — the play is still shown and simply ranks below the ones that measured well. n is how many hours were replayed; LOW COVERAGE means too few of them to trust the mean, not that the play is bad. Measured across 960 top-20 play-hours the ROI column overstates this by four to eight times, and that measurement is of DIRECT flips: a 1-hop route is replayed the same way but nothing has checked that a triangle behaves like a flip.',
	Investment:
		'Chaos you need liquid for ONE exchange — the cost at the undercut entry. What the whole worthwhile run ties up is in the Scale column, and that run figure is what the filter bar’s Run cost bounds are compared against: a bankroll ceiling there is about the trip, not about a single flip. Switch those bounds to divine for the large ones — converted at the divine/chaos rate from the same feed hour.',
	Gold: '(column hidden until computable) The in-game currency exchange charges gold per trade. Nothing here is net of it yet, and a reserved column of dashes promised a number the page could not give, so the column is gone until the per-trade cost is known and ROI can be shown net of gold.',
	Route:
		'The round trip as five slots: what you spend, the two or three trades, what you get back. Spend is the chaos ONE exchange costs at the undercut entry; Get is the chaos that same exchange returns — your spend plus the profit, not a separate payout. Each step’s rate is quoted in that step’s own currency, so a play that sells into divine shows a divine number there while both ends stay in chaos: the conversion back is already inside the Get figure. Both ends are per exchange — the Scale column says how many of them the play is worth repeating.',
	Trend:
		'Reserved for the fair-price trend across recent hours — whether the market this play trades against is drifting up or down. No per-play fair history is published yet, so nothing is derived and the cell shows a dash rather than a direction the data cannot support.',
	Depth:
		'Units per hour the thinnest of the play’s trades saw — the hourly ceiling. This is the whole market’s volume, not your share, and a direct play buys and sells the same item on one market, so its real ceiling is lower still. It is what the Scale column divides the worthwhile flip count by to answer how long the market needs.',
	Scale:
		'The flip count at which this play clears about 100 chaos of EXPECTED gain, what those flips tie up, and how long the market needs to absorb them. It counts Exp. ROI and not ROI, so the size is the one you would actually have had to run: a play whose expectation is 3c an exchange reads ×34, one worth 150c reads ×1, and a play that is not expected to gain at all shows a dash — there is no number of repeats that turns a loss into 100c. The hours are optimistic — they assume the WHOLE hourly volume on the play’s thinnest trade is yours and nobody else is competing for it — so read the wait as the floor on the time, not the time. Green is inside the hour, which is the only case where the ROI was computed against a book that will still be there.',
	Hours:
		'How many of the window’s hours this play held — the persistence count. An hour counts when the play was ALIVE in it (at least ten units of each traded item changed hands, with orders standing on both sides) and its return cleared the server’s +0.1% sanity floor. That is the whole test since POE-191: the quality bar moved to your Gates row, so these hours no longer ask a play to have been worth trading, only to have existed and paid. Expect the fraction to read higher than it used to. The server still needs a minimum before it serves a play at all — four of the recent window’s six hours, eighteen of the day window’s twenty-four — so the full count is a spread that stood all window and the minimum is one that only just persisted.',
	'Only / Hide':
		'Two layers. Category pills are coarse — the 16 buckets the in-game exchange lists down its own sidebar. Item chips are overrides: an item rule beats whatever its category says, and Hide beats Only when both apply. A play matches if either side of any of its trades hits a rule — what you buy or what you pay with. Both layers are remembered across restarts.',
	Mode: 'DIRECT buys and sells the same item on one market — market making, two trades. 1-HOP buys an item against one currency, sells it against another, then converts back — three trades, three chances to be beaten to the fill.',
	Suspect:
		'One of the play’s trades is priced outside its fair band: a buy below fair × 0.67, or a sell above fair × 1.5. The play is still served and still ranked, after every clean one, because the extreme may be a real fill or a single stray order and only the book can say which. Read the row as a signal, not a quote — verify the route in game before committing.',
	'Data age':
		'Every figure on this page is the last SETTLED feed hour, not the live book. The feed publishes 40–60 minutes after an hour closes, so these prices can be up to about two hours behind what the exchange is showing you right now. Check the route in game before committing to it.',
	'Run cost':
		'What the worthwhile run ties up — the Scale column’s investment, not the cost of one exchange. Set the min/max to see only what your bankroll covers; the divine toggle converts the bounds at the newest hour’s rate.',
	Gates:
		'The quality bar — four floors and one ceiling the SERVER used to apply to everybody before it sent anything (POE-191 handed them to you). All five now ship OFF: an empty box filters nothing, so the table shows everything the server served and the ranking does the judging. Each tooltip names the level worth typing if you want the old, stricter table back. Clear leaves all five alone; Defaults empties them, which is off.',
	'Min profit':
		'The least chaos a play must gain on ONE exchange, read off the ROI column and not off Exp. ROI — a gate asks whether the market is worth trading at all, never how far the play has to be repeated to pay, which is the Scale column’s answer. Empty (or 0) is off, which is how the cheap plays stay visible: sacrifice fragments and the like gain a fraction of a chaos each and only add up on volume. Type 3 for the old server floor.',
	'Min turnover':
		'How much chaos had to change hands on the play’s market during the feed hour. Empty (or 0) is off, so the quiet corners of the exchange are on the table. Type 10000 for the old server floor — below that you are not joining a market so much as being one, and your own order sets the price. It is a blunt line: a real flip turning over 8,500c an hour fails it.',
	'Max price step':
		'The coarsest price step the play’s market may quote in, as a percent of the price. A market that only moves in 25% jumps cannot be undercut finely, so the entry you planned is not the entry you get. Empty (or 0) means no ceiling. Type 10 for the old server level.',
	'Edge vs step':
		'How many price steps wide the play’s return has to be. At 5 the return must be five times the market’s own step, which keeps a play whose entire edge is one step of rounding off the table — but it also takes nearly every 1-hop route with it, because the divine step alone eats them. Empty (or 0) is off, which is why they are on the table. Type 5 for the old server level.',
	'Min return':
		'The least return a play must show, as a percent of what one exchange costs — the same NET figure the ROI% column prints. Empty (or 0) is off, which shows everything above the server’s +0.1% sanity floor — it never serves a play that loses money or gains only float noise. Type 2 for the floor the server used to apply to everyone.',
	Counter:
		'What is left of the response, and what took the rest. Gates are counted apart from everything else because their controls sit behind a collapsed row — the split points at the row that would give those rows back. They ship off, so this reads 0 until you set one. Everything else is counted together: the category pills, the item chips, the run-cost bounds, and the search box.'
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
