<script lang="ts">
	/**
	 * An item's inventory icon, from a URL the caller has already resolved
	 * (`$lib/exchange/view`'s `iconSrc` for Currency Exchange legs).
	 *
	 * Two different absences, deliberately rendered differently:
	 *
	 * - `src === null` — the server knows this item and has no artwork for it,
	 *   or does not know the id at all. Nothing is rendered, not even a
	 *   placeholder: a chip is readable as text, and a row of "?" boxes next to
	 *   names is noise rather than information.
	 * - the request FAILS (404 from the icon cache, upstream unreachable) — the
	 *   "?" glyph stands in, at exactly the image's box so the row does not
	 *   reflow when it appears. Same fallback as
	 *   `routes/(app)/components/GemIcon.svelte`.
	 *
	 * `width`/`height` are set from `size` on the element itself rather than
	 * left to CSS, so the space is reserved before the (lazily loaded) image
	 * decodes.
	 */
	let { src, alt, size = 18 }: { src: string | null; alt: string; size?: number } = $props();

	/**
	 * The src whose load failed, not a bare boolean: the chips re-render when
	 * the plays refresh, and a sticky flag would keep the fallback on screen for
	 * a *different* icon that was never tried.
	 */
	let failedSrc = $state<string | null>(null);
	let errored = $derived(failedSrc === src);
</script>

{#if src !== null}
	{#if errored}
		<span class="item-icon-fallback" style:width="{size}px" style:height="{size}px">?</span>
	{:else}
		<img
			{src}
			{alt}
			width={size}
			height={size}
			class="item-icon"
			loading="lazy"
			onerror={() => { failedSrc = src; }}
		/>
	{/if}
{/if}

<style>
	.item-icon {
		display: inline-block;
		vertical-align: middle;
		object-fit: contain;
		border-radius: 2px;
	}

	.item-icon-fallback {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		vertical-align: middle;
		background: rgba(156, 163, 175, 0.15);
		color: var(--color-lab-text-secondary);
		font-size: 0.75rem;
		border-radius: 2px;
	}
</style>
