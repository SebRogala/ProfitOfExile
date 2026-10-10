<script lang="ts">
	/**
	 * One stash-search regex: caption, the regex in a read-only input, a Copy
	 * button and "<n> / 250 characters", amber over the limit (README § Screens
	 * 7, references 01, 07 and 08 §8).
	 *
	 * Presentation only over `view.ts`'s `RegexBox`. The button's label follows
	 * `copied`; the click leaves through `oncopy` and the controller writes the
	 * clipboard and decides when "Copied" ends.
	 */
	import Button from '$lib/components/Button.svelte';
	import type { RegexBox } from './view';

	let {
		label,
		box,
		copied,
		oncopy
	}: { label: string; box: RegexBox; copied: boolean; oncopy: () => void } = $props();
</script>

<div class="box">
	<span class="caption">{label}</span>
	<div class="row">
		<input class="mono regex" type="text" readonly value={box.text} aria-label={label} />
		<span class="copy">
			<Button onclick={oncopy}>{copied ? 'Copied' : 'Copy'}</Button>
		</span>
	</div>
	<span class="count" class:over={box.overLimit}>{box.count}</span>
</div>

<style>
	.box {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.caption {
		font-size: 0.625rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--color-lab-text-secondary);
	}

	.row {
		display: flex;
		gap: 0.4rem;
	}

	.regex {
		flex: 1;
		min-width: 0;
		padding: 0.3rem 0.5rem;
		background: var(--color-lab-bg);
		border: 1px solid var(--color-lab-border);
		border-radius: 3px;
		color: var(--color-lab-text);
		font-family: 'Consolas', 'Monaco', monospace;
		font-weight: 600;
	}

	.regex:focus-visible {
		outline: 2px solid var(--color-lab-blue);
		outline-offset: 1px;
	}

	.copy :global(.btn:focus-visible) {
		outline: 2px solid var(--color-lab-blue);
		outline-offset: 1px;
	}

	.count {
		font-size: 0.75rem;
		color: var(--color-lab-text-secondary);
	}

	.count.over {
		color: var(--color-lab-yellow);
	}
</style>
