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
 * what one round trip costs, net of the undercut each leg pays. Pointing a gem
 * tooltip at this table would word the number wrongly by a factor of a hundred.
 *
 * Every figure these describe is the LAST SETTLED feed hour (POE-188), not the
 * live book — which is why each entry ends where it does: with what to do about
 * that, rather than with the number as if it were a quote.
 */
export const EXCHANGE_TOOLTIPS: Record<string, string> = {
	ROI: 'Absolute profit in chaos orbs — one exchange is chaos in, chaos out. What you get back minus what you spent, multiplied by your Quantity; the sub-line is the per-exchange figure. Both are net of one tick of undercut on each leg, the price an order that actually fills pays. Higher = more profit, but every figure here is the last settled hour: verify the route in game before committing.',
	'ROI%':
		'Return on investment as a percentage — ROI divided by what one exchange costs. Scale-free, so it compares plays across price tiers. NET is net of one tick of undercut on every leg, the return an order that actually gets taken can expect, and it is what the ranking uses. RAW is the same round trip at the hour’s raw extremes — never below NET, and the gap between them is what the ticks cost.',
	Quantity:
		'How many exchanges you intend to run. ROI and Investment are both multiplied by it; ROI% is not, because it is scale-free. Default 1, and it is remembered across restarts. Set it above a play’s Depth and the Depth cell turns amber — the number is never capped for you.',
	Investment:
		'Chaos you need liquid: the per-exchange cost at the undercut entry, times your Quantity. Set the min/max in the filter bar to see only what your bankroll covers, and switch the unit to divine for the large ones — converted at the divine/chaos rate from the same feed hour.',
	Gold: '(column hidden until computable) The in-game currency exchange charges gold per trade. Nothing here is net of it yet, and a reserved column of dashes promised a number the page could not give, so the column is gone until the per-trade cost is known and ROI can be shown net of gold.',
	Route:
		'The round trip as five slots: what you spend, the two or three trades, what you get back. Spend is the chaos ONE exchange costs at the undercut entry; Get is the chaos that same exchange returns — your spend plus the profit, not a separate payout. Each step’s rate is quoted in that leg’s own currency, so a play that sells into divine shows a divine number there while both ends stay in chaos: the conversion back is already inside the Get figure. Both ends are per exchange, whatever your Quantity.',
	Fill: 'How long trading your Quantity would take at this play’s Depth — Quantity ÷ units per hour, rounded up, green under an hour. Optimistic on purpose: it assumes you take the market’s WHOLE hourly volume on the thinnest leg, which everyone else is competing for, and a direct play buys and sells on that one market. Read it as the floor on the time, not the time.',
	Trend:
		'Reserved for the fair-price trend across recent hours — whether the market this play trades against is drifting up or down. No per-play fair history is published yet, so nothing is derived and the cell shows a dash rather than a direction the data cannot support.',
	Depth:
		'Units per hour the thinnest leg traded — the hourly ceiling. This is the whole market’s volume, not your share, and a direct play buys and sells the same item on one market, so its real ceiling is lower still. A Quantity above it is marked amber rather than capped: the ROI stands, filling it takes longer than an hour or moves the price.',
	Hours:
		'How many of the window’s hours this play held — the persistence gate. A play must clear every gate in at least four of the recent window’s six hours (eighteen of the day window’s twenty-four) to be served at all, so the full count is a standing spread and the minimum is an edge that only just persisted.',
	'Only / Hide':
		'Two layers. Category pills are coarse — the 16 buckets the in-game exchange lists down its own sidebar. Item chips are overrides: an item rule beats whatever its category says, and Hide beats Only when both apply. A play matches if any leg’s item or quote hits a rule, in either role. Both layers are remembered across restarts.',
	Mode: 'DIRECT buys and sells the same item on one market — market making, two trades. 1-HOP buys an item against one currency, sells it against another, then converts back — three trades, three chances to be beaten to the fill.',
	Suspect:
		'A leg’s price sits outside its fair band: a buy below fair × 0.67, or a sell above fair × 1.5. The play is still served and still ranked, after every clean one, because the extreme may be a real fill or a single stray order and only the book can say which. Read the row as a signal, not a quote — verify the route in game before committing.',
	'Data age':
		'Every figure on this page is the last SETTLED feed hour, not the live book. The feed publishes 40–60 minutes after an hour closes, so these prices can be up to about two hours behind what the exchange is showing you right now. Check the route in game before committing to it.',
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
