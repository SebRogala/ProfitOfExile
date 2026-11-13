<script lang="ts">
	import { notice as bundledNotice, type Notice } from '$lib/privacy/notice';

	let { notice = bundledNotice }: { notice?: Notice } = $props();
</script>

<div id="privacy-notice">
	{#each notice.sections as section (section.id)}
		<section data-section={section.id}>
			<h4>{section.heading}</h4>
			{#each section.paragraphs as paragraph}<p>{paragraph}</p>{/each}
			{#if section.items.length > 0}
				<ul>{#each section.items as item}<li>{item}</li>{/each}</ul>
			{/if}
		</section>
	{/each}
</div>

<style>
	h4 {
		margin: 0.75rem 0 0.25rem;
		font-size: 0.85rem;
	}

	p, ul {
		margin: 0.25rem 0;
		color: var(--text-muted);
	}
</style>
