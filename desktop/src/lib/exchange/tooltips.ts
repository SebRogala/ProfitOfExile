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
 * Prices use settled feed data, with trailing-window prices marked on the
 * page. Expected profit is simulated across hours, not an executable quote;
 * Hours counts persistence across the selected window (ADR-016).
 *
 * Written for a reader who plays the game, not one who trades for a living
 * (POE-191): a play is a round trip made of TRADES or STEPS, never of "legs".
 * The word survives in the code, where it names a wire field, and nowhere the
 * reader can see it.
 */
export const NO_OFFERS_LABEL = 'no offers';

export const EXCHANGE_TOOLTIPS: Record<string, string> = {
	ROI:
		'Best-case chaos profit at the row’s size, after undercutting each trade by one price step.',
	'ROI%':
		'Return as a percentage of Investment: NET includes one price step of undercut per trade; RAW uses the printed extremes.',
	'Exp. ROI':
		'<b>Simulated chaos profit at the row’s size</b><br><br><b>Result</b> (mean over the last day, at row size):<br>• <span style=\'color:var(--color-lab-green)\'>Gain</span> — Positive expectation at the displayed order size.<br>• <span style=\'color:var(--color-lab-red)\'>Loss</span> — Negative expectation; repeats cannot make it profitable.<br>• 0c — Flat or rounded from a fractional result.<br>• <span style=\'color:#4b5563\'>—</span> — No hour could be simulated.<br><br><b>Coverage</b> (hours replayed):<br>• <span style=\'color:#6b7280\'>n</span> — Number of simulated entry hours.<br>• <span style=\'color:var(--color-lab-yellow)\'>low</span> — Too few hours to trust the mean.',
	Investment:
		'Chaos needed for one entry-market order, or one item when no order quantity is available. For the run’s cost, see Scale.',
	Gold: '(column hidden until computable) The in-game currency exchange charges gold per trade. Nothing here is net of it yet, and a reserved column of dashes promised a number the page could not give, so the column is gone until the per-trade cost is known and ROI can be shown net of gold.',
	Route:
		'<b>Route at one entry-market order</b><br><br><b>Size</b> (what this row counts):<br>• Order — One entry-market posting; one item without quantity data.<br><br><b>Steps</b> (read left to right):<br>• Spend — Entry cost in the currency you pay.<br>• Buy — Item bought.<br>• Sell — Item sold.<br>• Convert — 1-HOP only: converts sale proceeds back.<br>• Get — Spend plus simulated profit, including losses.<br><br><b>Prices</b> (totals where available):<br>• ≈ — Undercut total; hover for market quantities.<br>• Last step — Spend plus ROI; can differ from Get.<br>• Market ratio — No total available; hover for sizing caveat.',
	Trend:
		'Fair-price trend is unavailable; the dash has no directional reading.',
	Depth:
		'Hourly units traded on the route’s thinnest market: the whole market’s volume, not your share. For run time, see Scale.',
	Scale:
		'<b>Run targeting ≥100c of expected gain</b><br><br><b>Run</b> (whole-run figures):<br>• ×N — Exchanges needed, based on simulated profit.<br>• +Nc — Expected gain across those exchanges.<br>• Nc in — Chaos tied up; used by Run cost.<br>• <span style=\'color:#4b5563\'>—</span> — No profitable run can be derived.<br><br><b>Wait</b> (assuming all market volume is yours):<br>• <span style=\'color:var(--color-lab-green)\'>≤1 h</span> — Run fits within one feed hour.<br>• <span style=\'color:var(--color-lab-yellow)\'>≥2 h</span> — Minimum wait; competition adds time.<br>• <span style=\'color:#4b5563\'>— h</span> — Hourly wait cannot be derived.',
	Hours:
		'Hours the route was priced within the selected window; the bar is green only for a full window.',
	'Only / Hide':
		'Category pills filter by exchange category; item chips override them, and Hide beats Only.',
	Mode:
		'Trade count: DIRECT buys and sells on one market; 1-HOP adds currency conversion back to the starting currency.',
	Suspect:
		'A trade priced outside its fair band: buy below 0.67× fair or sell above 1.5× fair.',
	'Depleted side':
		'One side of this step\'s book was empty in the hour.',
	'Low liquidity':
		'The newest hour\'s undercut round trip returned less than the server floor; Exp. ROI still ranks the row.',
	'Window priced':
		'Prices are the cheapest and dearest trades over the last N hours; the newest hour printed one side only.',
	'Step not used':
		'The convert step a DIRECT play does not take.',
	'Expected gain':
		'Green Get: positive expected profit; a loss prints no green and reads "lose ≈".',
	'Measured loss':
		'Red Exp. ROI: the last day\'s replay came out negative.',
	'Low coverage':
		'Dimmed Exp. ROI marked "low": too few hours replayed to trust the mean.',
	'No reading':
		'A dash means this row has no value here, never zero.',
	'Full window':
		'Full green bar: the play held every hour of the window; see Hours.',
	'Data age':
		'Last settled feed hour, published 40–60 min after close; up to ~2 h behind the live book.',
	'Run cost':
		'Bounds on what the worthwhile run ties up; see Scale\'s "Nc in".',
	Gates:
		'Seven quality gates; only the two Min item price floors ship on.',
	'Min item price':
		'Least chaos one exchange may cost to enter (ships at 0.5; 0 disables).',
	'Min item price (div)':
		'Least divine one exchange may cost to enter, divine-entry plays only (ships at 0.4; 0 disables).',
	'Min profit':
		'Least best-case chaos one exchange must gain; the old server floor was 3.',
	'Min turnover':
		'Least chaos the play\'s market traded in the feed hour; the old server floor was 10000.',
	'Max price step':
		'Coarsest price step the market may quote, as % of price; the old server level was 10.',
	'Edge vs step':
		'Least return as a multiple of the market\'s price step; the old server level was 5.',
	'Min return':
		'Least NET return per exchange, as ROI% prints it; the old server floor was 2.',
	Counter:
		'Plays shown of plays served; rows hidden by gates and by other filters are counted apart.'
};
