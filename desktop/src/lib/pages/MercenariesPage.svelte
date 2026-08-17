<script lang="ts">
	import {
		MERC_SOURCES,
		entryKind,
		type MercFilterGroup,
		type MercTier,
	} from '$lib/mercenaries/rulesets';
	import { savedSearchUrl } from '$lib/mercenaries/trade-links';
	import { ssot } from '$lib/stores/ssot.svelte';

	// Every rule shown here is read from `$lib/mercenaries/rulesets` — the page
	// renders the rulesets, it never re-derives them. In particular the chip
	// colour comes from `entryKind`, which resolves type-first (a switched-off
	// denial is still a denial); reproducing that rule in markup is how the page
	// would end up advertising the stats a search exists to reject.

	const TIER_LABELS: Record<MercTier, string> = {
		mv: 'minimum viable',
		mid: 'mid',
		end: 'endgame',
		gg: 'GG',
	};

	/** Entries the saved search actually has switched on, ignoring the group switch. */
	function enabledEntries(group: MercFilterGroup) {
		return group.entries.filter((e) => e.enabledInSearch);
	}

	/**
	 * How many of the group's entries a mercenary has to satisfy, in words.
	 *
	 * An absent `min` means "all enabled entries must match" on a positive group
	 * and "none of these may be present" on a `not` group — the trade site's own
	 * semantics, which the raw number alone does not carry.
	 */
	function quantifier(group: MercFilterGroup): string {
		if (enabledEntries(group).length === 0) return 'bonus vocabulary — none enabled';
		if (group.type === 'not') return 'none of:';
		// A switched-off positive group is not applying its requirement — saying
		// "all of:" under an "off in this search" badge would claim it is. (A
		// switched-off `not` group still reads "none of:" — a parked denial is
		// still a denial, same type-first rule as entryKind.)
		if (!group.enabledInSearch) return 'when enabled:';
		if (group.min !== undefined) return `at least ${group.min} of:`;
		return 'all of:';
	}

	/** Chip tooltip naming the kind in words — colour and opacity alone must
	 *  not be the only carriers (colourblind + screen-reader parity). */
	function kindTitle(kind: 'required' | 'forbidden' | 'bonus'): string {
		if (kind === 'required') return 'required';
		if (kind === 'forbidden') return 'denied';
		return 'bonus — off in this search';
	}

	/**
	 * True when the entry is live in the search — an entry inside a switched-off
	 * group is off whatever its own flag says.
	 */
	function isLive(group: MercFilterGroup, entry: { enabledInSearch: boolean }): boolean {
		return group.enabledInSearch && entry.enabledInSearch;
	}

	/**
	 * The league the SSOT resolved, or null while it is still resolving. Null is
	 * "not known yet", not "different" — comparing against it would flash a
	 * mismatch badge on every cold start.
	 */
	function isOtherLeague(league: string): boolean {
		return ssot.league !== null && ssot.league !== league;
	}
</script>

<div class="merc-page">
	<h1>Mercenaries</h1>
	<p class="intro">
		Each ruleset below is a transcription of a saved trade search, switches included.
		Rows the search has switched off are shown dimmed — they are what the guide
		treats as upside, not as a requirement.
	</p>

	<section class="verdict">
		<h2>Last capture</h2>
		<p class="stub">
			No capture yet. The Merc OCR module (slice 2) fills this with per-rule
			pass/fail for the last seen mercenary.
		</p>
	</section>

	{#each MERC_SOURCES as source (source.id)}
		<section class="source">
			<h2>
				{source.label}
				{#if source.guideUrl}
					<a class="guide-link" href={source.guideUrl} target="_blank">guide</a>
				{/if}
			</h2>

			{#each source.rulesets as ruleset (ruleset.id)}
				<article class="ruleset">
					<header class="ruleset-head">
						<span class="ruleset-label">{ruleset.label}</span>
						{#if ruleset.tier}
							<span class="tier">{TIER_LABELS[ruleset.tier]}</span>
						{/if}
						<a
							class="search-link"
							href={savedSearchUrl(ruleset.savedSearch)}
							target="_blank"
							aria-label="open saved search — {source.label} {ruleset.label}{ruleset.tier ? ` ${TIER_LABELS[ruleset.tier]}` : ''}"
						>
							open saved search
						</a>
						{#if isOtherLeague(ruleset.savedSearch.league)}
							<span class="league-badge">saved in {ruleset.savedSearch.league}</span>
						{/if}
					</header>

					<div class="ruleset-meta">
						<span>status: {ruleset.status}</span>
						{#if ruleset.ilvlMin !== undefined}
							<span>ilvl {ruleset.ilvlMin}+</span>
						{/if}
					</div>

					{#if ruleset.floor}
						<p class="floor">Floor: {ruleset.floor}</p>
					{/if}

					{#each ruleset.groups as group (group.id)}
						<div class="group" class:group-off={!group.enabledInSearch}>
							<div class="group-head">
								<span class="group-label">{group.label}</span>
								{#if !group.enabledInSearch}
									<span class="off-badge">off in this search</span>
								{/if}
								<span class="quantifier">{quantifier(group)}</span>
							</div>
							<div class="chips">
								{#each group.entries as entry (group.id + entry.id)}
									{@const kind = entryKind(group, entry)}
									<span
										class="chip chip-{kind}"
										class:chip-off={!isLive(group, entry)}
										title={kindTitle(kind)}
									>
										<span class="marker" aria-hidden="true"></span>
										{entry.name}
									</span>
								{/each}
							</div>
						</div>
					{/each}
				</article>
			{/each}
		</section>
	{/each}
</div>

<style>
	.merc-page {
		max-width: 900px;
		margin: 0 auto;
		color: var(--color-lab-text);
	}

	h1 {
		font-size: 1.2rem;
		color: var(--color-lab-text);
		margin-bottom: 0.5rem;
	}

	.intro {
		font-size: 0.8rem;
		color: var(--color-lab-text-secondary);
		margin-bottom: 1rem;
		max-width: 68ch;
	}

	section {
		background: var(--color-lab-surface);
		border: 1px solid var(--color-lab-border);
		border-radius: 8px;
		padding: 1rem;
		margin-bottom: 1rem;
	}

	h2 {
		font-size: 0.8rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-lab-text-muted);
		margin-bottom: 0.75rem;
	}

	.stub {
		font-size: 0.8rem;
		color: var(--color-lab-text-secondary);
	}

	.guide-link,
	.search-link {
		font-size: 0.7rem;
		text-transform: none;
		letter-spacing: 0;
		color: var(--color-lab-blue);
		text-decoration: none;
	}

	.guide-link:hover,
	.search-link:hover {
		text-decoration: underline;
	}

	.ruleset {
		border-top: 1px solid var(--color-lab-border);
		padding-top: 0.75rem;
		margin-top: 0.75rem;
	}

	.ruleset:first-of-type {
		border-top: none;
		padding-top: 0;
		margin-top: 0;
	}

	.ruleset-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.ruleset-label {
		font-size: 0.95rem;
		font-weight: 600;
	}

	.tier {
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-purple);
		border: 1px solid var(--color-lab-purple);
		border-radius: 999px;
		padding: 1px 7px;
	}

	.league-badge {
		font-size: 0.65rem;
		color: var(--color-lab-yellow);
		border: 1px solid var(--color-lab-yellow);
		border-radius: 999px;
		padding: 1px 7px;
	}

	.ruleset-meta {
		display: flex;
		gap: 0.75rem;
		font-size: 0.7rem;
		color: var(--color-lab-text-muted);
		margin-top: 0.25rem;
	}

	.floor {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
		margin-top: 0.25rem;
	}

	.group {
		margin-top: 0.6rem;
	}

	.group-off {
		opacity: 0.65;
	}

	/* The group's own dimming already carries "off"; letting the per-chip 0.7
	 * multiply under it lands red text at ~1.5:1 contrast — the Pierce denial
	 * list the type-first kind rule exists to keep visible would vanish. */
	.group-off .chip-off {
		opacity: 1;
	}

	.group-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-bottom: 0.3rem;
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

	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
	}

	/* Read-only chips: the colour is always on (it encodes the rule, not a
	   selection), so there is no hover or active state to hunt for. */
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.72rem;
		line-height: 1.5;
		padding: 2px 8px;
		border-radius: 4px;
		border: 1px solid;
	}

	.marker {
		width: 8px;
		height: 8px;
		border-radius: 2px;
		border: 1px solid currentColor;
		background: currentColor;
		flex-shrink: 0;
	}

	/* Switched off in the search: dimmed, with the marker hollowed out the way an
	   unchecked box reads on the trade site. 0.7 is the floor that keeps the
	   coloured text above ~3:1 on the lab surface — do not lower it. */
	.chip-off {
		opacity: 0.7;
	}

	.chip-off .marker {
		background: transparent;
	}

	.chip-required {
		color: var(--color-lab-green);
		border-color: var(--color-lab-green);
		background: rgba(34, 197, 94, 0.1);
	}

	.chip-forbidden {
		color: var(--color-lab-red);
		border-color: var(--color-lab-red);
		background: rgba(239, 68, 68, 0.1);
	}

	.chip-bonus {
		color: var(--color-lab-blue);
		border-color: var(--color-lab-blue);
		background: rgba(59, 130, 246, 0.1);
	}
</style>
