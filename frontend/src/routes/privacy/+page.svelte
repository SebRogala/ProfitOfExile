<script lang="ts">
	import { notice } from '$lib/privacy/notice-data';
</script>

<svelte:head>
	<title>Data & Privacy | ProfitOfExile</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<main class="min-h-screen bg-lab-bg px-4 py-10 text-lab-text">
	<article class="mx-auto max-w-3xl">
		<header class="mb-8 border-b border-lab-border pb-6">
			<a href="/" class="text-sm text-poe-gold hover:underline">ProfitOfExile</a>
			<div class="mt-3 flex flex-wrap items-center gap-3">
				<h1 class="text-3xl font-semibold text-poe-light">Data & Privacy</h1>
				{#if notice.draft}
					<span class="rounded border border-lab-yellow px-2 py-0.5 text-xs font-medium uppercase tracking-wide text-lab-yellow">Draft</span>
				{/if}
			</div>
			<p class="mt-2 text-sm text-lab-text-secondary">
				Revision <span data-testid="notice-revision">{notice.revision}</span> · {notice.date}
			</p>
		</header>

		{#each notice.sections as section (section.id)}
			<section id={section.id} class="mb-8">
				<h2 class="mb-3 text-xl font-semibold text-poe-gold">{section.heading}</h2>
				{#each section.paragraphs as paragraph, i (i)}
					<p class="mb-3 leading-relaxed">{paragraph}</p>
				{/each}
				{#if section.items.length > 0}
					<ul class="list-disc space-y-1 pl-6">
						{#each section.items as item, i (i)}
							<li>{item}</li>
						{/each}
					</ul>
				{/if}
			</section>
		{/each}
	</article>
</main>
