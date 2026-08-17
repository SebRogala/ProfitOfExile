<script lang="ts">
	import {
		MERC_SOURCES,
		entryKind,
		kinetistLadder,
		type MercRuleset,
		type MercSourceId,
	} from '$lib/mercenaries/rulesets';
	import {
		columnLabel,
		kindTitle,
		ladderRows,
		quantifier,
		sharedValue,
		type LadderCell,
	} from '$lib/mercenaries/ladder-view';
	import { savedSearchUrl } from '$lib/mercenaries/trade-links';
	import SegmentedButtons from '$lib/components/SegmentedButtons.svelte';
	import { ssot } from '$lib/stores/ssot.svelte';

	// Every rule shown here is read from `$lib/mercenaries/rulesets` — the page
	// renders the rulesets, it never re-derives them. In particular the glyph and
	// its colour come from `entryKind`, which resolves type-first (a switched-off
	// denial is still a denial); reproducing that rule in markup is how the page
	// would end up advertising the stats a search exists to reject.

	/** One glyph per state. Text, not shapes: it survives a colour-blind reader,
	 *  and the wording behind it is on the row's `title`. */
	const GLYPH: Record<LadderCell, string> = {
		required: '✓',
		forbidden: '✕',
		bonus: '＋',
		absent: '·',
	};

	const SOURCE_OPTIONS = MERC_SOURCES.map((s) => ({ value: s.id, label: s.label }));

	let selectedSource = $state<MercSourceId>(MERC_SOURCES[0].id);

	const source = $derived(MERC_SOURCES.find((s) => s.id === selectedSource) ?? MERC_SOURCES[0]);

	/**
	 * The tier matrix is guide-b's presentation, not a generic capability — a
	 * future source with rungs of its own gets this treatment when it exists, not
	 * speculatively now.
	 */
	const ladder = $derived(
		source.id === 'guide-b' ? kinetistLadder().filter((r) => source.rulesets.includes(r)) : []
	);
	const rows = $derived(ladderRows(ladder));
	const cards = $derived(source.rulesets.filter((r) => !ladder.includes(r)));

	/** Status + item level as one line — identical across all four rungs today, so
	 *  the matrix prints it once instead of four times. */
	function metaText(ruleset: MercRuleset): string {
		return ruleset.ilvlMin === undefined
			? ruleset.status
			: `${ruleset.status} · ilvl ${ruleset.ilvlMin}+`;
	}

	const ladderMeta = $derived(sharedValue(ladder.map(metaText)));

	/** Wording for a matrix cell — `absent` is a hole in the skeleton, not a rule. */
	function cellTitle(cell: LadderCell): string {
		return cell === 'absent' ? 'not in this rung' : kindTitle(cell);
	}

	/**
	 * Leagues this source's saved searches live in that are NOT the league the SSOT
	 * resolved. A null `ssot.league` is "not known yet", not "different" — comparing
	 * against it would flash a mismatch badge on every cold start.
	 */
	function otherLeagues(rulesets: MercRuleset[]): string[] {
		if (ssot.league === null) return [];
		const league = ssot.league;
		return [...new Set(rulesets.map((r) => r.savedSearch.league))].filter((l) => l !== league);
	}
</script>

<div class="merc-page">
	<div class="page-head">
		<h1>Mercenaries</h1>
		<SegmentedButtons
			value={selectedSource}
			options={SOURCE_OPTIONS}
			onselect={(id) => (selectedSource = id as MercSourceId)}
			title="Which guide's rules to show. Each is a transcription of that guide's saved trade searches."
		/>
	</div>

	<p class="intro">
		Each ruleset is a transcription of a saved trade search, switches included.
		<span class="legend">
			<span class="legend-item entry-required"
				><span class="glyph" aria-hidden="true">{GLYPH.required}</span> required</span
			>
			<span class="legend-item entry-forbidden"
				><span class="glyph" aria-hidden="true">{GLYPH.forbidden}</span> denied</span
			>
			<span class="legend-item entry-bonus"
				><span class="glyph" aria-hidden="true">{GLYPH.bonus}</span> bonus — switched off, so
				upside rather than a requirement</span
			>
		</span>
	</p>

	<section class="card verdict">
		<h2 class="card-title">Last capture</h2>
		<p class="stub">
			No capture yet. The Merc OCR module (slice 2) fills this with per-rule
			pass/fail for the last seen mercenary.
		</p>
	</section>

	<section class="panel">
		<header class="panel-head">
			<h2>{source.label}</h2>
			{#if source.guideUrl}
				<a class="guide-link" href={source.guideUrl} target="_blank">guide</a>
			{:else}
				<!-- Provenance is pending, not nonexistent — the URL is a known gap. -->
				<span class="guide-pending">guide URL pending</span>
			{/if}
			{#each otherLeagues(source.rulesets) as league (league)}
				<span class="league-badge">saved in {league}</span>
			{/each}
		</header>

		<div class="grid">
			{#if rows.length > 0}
				<article class="card matrix-card">
					<header class="card-head">
						<h3 class="card-title">Kinetist — tier ladder</h3>
						{#if ladderMeta}<span class="meta">{ladderMeta}</span>{/if}
					</header>
					<p class="matrix-hint">
						One search, four rungs. Highlighted rows are the ones that move between them.
					</p>

					<!-- Never wraps: the columns hold their width and the card scrolls. -->
					<div class="matrix-scroll">
						<table class="matrix">
							<thead>
								<tr>
									<th class="rule-col" scope="col">rule</th>
									{#each ladder as rung (rung.id)}
										<th class="tier-col" scope="col">
											<span class="tier-name">{columnLabel(rung)}</span>
											<a
												class="search-link"
												href={savedSearchUrl(rung.savedSearch)}
												target="_blank"
												aria-label="open saved search — {source.label} {rung.label} {columnLabel(
													rung
												)}"
											>
												↗
											</a>
											{#if !ladderMeta}<span class="meta">{metaText(rung)}</span>{/if}
											{#if rung.floor}<span class="floor">{rung.floor}</span>{/if}
										</th>
									{/each}
								</tr>
							</thead>
							<tbody>
								{#each rows as row (row.id)}
									{#if row.kind === 'group'}
										<tr class="group-row" class:varies={row.varies}>
											<th class="group-cell" colspan={ladder.length + 1} scope="colgroup">
												<span class="group-label">{row.label}</span>
												<span class="quantifier">{row.quantifier}</span>
												{#if row.offIn.length > 0}
													<span class="off-badge">off in {row.offIn.join(', ')}</span>
												{/if}
											</th>
										</tr>
									{:else}
										<tr class="entry-row" class:varies={row.varies}>
											<th class="name-cell" scope="row">
												{row.name}
												{#if row.varies}
													<!-- The tint is the visual carrier; this is the same fact
													     for readers the tint cannot reach. -->
													<span class="visually-hidden">(varies between rungs)</span>
												{/if}
											</th>
											{#each row.cells as cell, i (ladder[i].id)}
												<td
													class="cell entry-{cell}"
													title={cellTitle(cell)}
													aria-label={cellTitle(cell)}
												>
													<span class="glyph" aria-hidden="true">{GLYPH[cell]}</span>
												</td>
											{/each}
										</tr>
									{/if}
								{/each}
							</tbody>
						</table>
					</div>
				</article>
			{/if}

			{#each cards as ruleset (ruleset.id)}
				<article class="card">
					<header class="card-head">
						<h3 class="card-title">{ruleset.label}</h3>
						<a
							class="search-link"
							href={savedSearchUrl(ruleset.savedSearch)}
							target="_blank"
							aria-label="open saved search — {source.label} {ruleset.label}"
						>
							open saved search ↗
						</a>
					</header>
					<div class="meta">{metaText(ruleset)}</div>
					{#if ruleset.floor}
						<p class="floor">Floor: {ruleset.floor}</p>
					{/if}

					{#each ruleset.groups as group (group.id)}
						<div class="group">
							<div class="group-head">
								<span class="group-label">{group.label}</span>
								<span class="quantifier">{quantifier(group)}</span>
								{#if !group.enabledInSearch}
									<span class="off-badge">off in this search</span>
								{/if}
							</div>
							<ul class="entries">
								<!-- Keys are unique per group (asserted upstream in rulesets.test.ts);
								     the same id may legitimately recur across sibling groups. -->
								{#each group.entries as entry (entry.id)}
									{@const kind = entryKind(group, entry)}
									<li class="entry entry-{kind}" title={kindTitle(kind)}>
										<span class="glyph" aria-hidden="true">{GLYPH[kind]}</span>
										<span class="entry-name">{entry.name}</span>
									</li>
								{/each}
							</ul>
						</div>
					{/each}
				</article>
			{/each}
		</div>
	</section>
</div>

<style>
	.merc-page {
		max-width: 1400px;
		margin: 0 auto;
		color: var(--color-lab-text);
	}

	.page-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin-bottom: 0.5rem;
	}

	h1 {
		font-size: 1.2rem;
		color: var(--color-lab-text);
	}

	.intro {
		font-size: 0.8rem;
		color: var(--color-lab-text-secondary);
		margin-bottom: 1rem;
	}

	.legend {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 0.9rem;
		margin-left: 0.5rem;
	}

	.legend-item {
		white-space: nowrap;
	}

	.card,
	.panel {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		border-radius: 8px;
		padding: 1rem;
	}

	.verdict {
		margin-bottom: 1rem;
	}

	.stub {
		font-size: 0.8rem;
		color: var(--color-lab-text-secondary);
	}

	/* The source you are inside is the loudest thing on the page — it used to be a
	   muted banner under headings that shouted over it. */
	.panel-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.6rem;
		margin-bottom: 0.75rem;
	}

	.panel-head h2 {
		font-size: 1.05rem;
		font-weight: 600;
		color: var(--color-lab-text);
	}

	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(340px, 100%), 1fr));
		gap: 0.75rem;
		align-items: start;
	}

	/* The matrix is the source's headline, and it is wider than one grid track. */
	.matrix-card {
		grid-column: 1 / -1;
	}

	.card-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.card-title {
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--color-lab-text);
	}

	.guide-link,
	.search-link {
		font-size: 0.7rem;
		color: var(--color-lab-blue);
		text-decoration: none;
	}

	.guide-link:hover,
	.search-link:hover {
		text-decoration: underline;
	}

	.guide-link:focus-visible,
	.search-link:focus-visible {
		outline: 1px solid var(--color-lab-blue);
		outline-offset: 2px;
	}

	.visually-hidden {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
	}

	.guide-pending {
		font-size: 0.7rem;
		color: var(--color-lab-text-secondary);
		font-style: italic;
	}

	.league-badge {
		font-size: 0.65rem;
		color: var(--color-lab-yellow);
		border: 1px solid var(--color-lab-yellow);
		border-radius: 999px;
		padding: 1px 7px;
	}

	.meta {
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
	}

	.floor {
		font-size: 0.72rem;
		color: var(--color-lab-text-secondary);
	}

	.matrix-hint {
		font-size: 0.72rem;
		color: var(--color-lab-text-muted);
		margin: 0.15rem 0 0.5rem;
	}

	.matrix-scroll {
		overflow-x: auto;
		/* `overflow-x: auto` alone COMPUTES overflow-y to auto as well (the spec
		 * forbids visible on one axis with non-visible on the other), and on
		 * Windows classic scrollbars any horizontal bar's own height then spawns
		 * a spurious vertical bar — the double-scrollbar Sebastian hit. The
		 * matrix never needs to scroll vertically; its height is its content. */
		overflow-y: hidden;
	}

	.matrix {
		border-collapse: collapse;
		width: 100%;
	}

	.matrix th,
	.matrix td {
		text-align: left;
		vertical-align: top;
		padding: 2px 8px;
		font-weight: 400;
	}

	.rule-col {
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-muted);
		min-width: 260px;
	}

	.tier-col {
		min-width: 92px;
		border-bottom: 1px solid var(--color-lab-border);
		padding-bottom: 0.35rem;
	}

	.tier-name {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-lab-purple);
		margin-right: 0.3rem;
	}

	.tier-col .meta,
	.tier-col .floor {
		display: block;
	}

	.group-row .group-cell {
		padding-top: 0.7rem;
	}

	.entry-row .name-cell {
		font-size: 0.75rem;
		line-height: 1.5;
		color: var(--color-lab-text);
	}

	.cell {
		line-height: 1.5;
	}

	/* The deltas are what a reader comparing rungs is hunting for; everything else
	   is the skeleton they share. */
	.varies th,
	.varies td {
		background: rgba(168, 85, 247, 0.08);
	}

	.group {
		margin-top: 0.6rem;
	}

	.group-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-bottom: 0.15rem;
	}

	.group-label {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-lab-text);
	}

	.off-badge {
		font-size: 0.6rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-lab-text-muted);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		padding: 0 4px;
	}

	.quantifier {
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
	}

	/* Rows, not pills: no border, no fill, no per-row padding — the glyph column is
	   the only structure, so a long list scans as a list. */
	.entries {
		list-style: none;
		padding-left: 0.15rem;
	}

	.entry {
		display: flex;
		align-items: baseline;
		gap: 0.4rem;
		font-size: 0.75rem;
		line-height: 1.5;
	}

	.glyph {
		display: inline-block;
		width: 0.9em;
		text-align: center;
		flex-shrink: 0;
	}

	.entry-name {
		color: var(--color-lab-text);
	}

	.entry-required .glyph {
		color: var(--color-lab-green);
	}

	.entry-forbidden .glyph {
		color: var(--color-lab-red);
	}

	.entry-bonus .glyph {
		color: var(--color-lab-blue);
	}

	/* Colour IS the dimming for a bonus row — stacking opacity on top of a coloured
	   glyph is what dropped the parked Pierce denials to ~1.5:1 in the pill design. */
	.entry-bonus .entry-name {
		color: var(--color-lab-text-secondary);
	}

	.entry-absent .glyph {
		color: var(--color-lab-text-muted);
	}
</style>
