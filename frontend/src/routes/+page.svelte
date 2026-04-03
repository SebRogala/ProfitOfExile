<script lang="ts">
	let currentFeature = $state(0);
	let zoomedImg = $state<string | null>(null);
	const features = [
		{
			title: 'OCR Gem Detection',
			desc: 'Hover over Font options — the app reads gem names directly from your screen and compares them instantly.',
			icon: 'M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z',
		},
		{
			title: 'Live Trade Prices',
			desc: 'Direct GGG trade API lookups from your machine. See real listings, seller concentration, price outliers.',
			icon: 'M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z',
		},
		{
			title: 'In-Game Overlay',
			desc: 'Transparent overlay with gem comparison, pick buttons, and trade data — fully click-through, never blocks the game.',
			icon: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z',
		},
		{
			title: 'Font Craft Tracking',
			desc: 'Automatically captures craft options, tracks remaining uses, detects jackpots. Session data sent to server for analysis.',
			icon: 'M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z',
		},
		{
			title: 'Lab Map Overlays',
			desc: 'Compass and path strip overlays show your route through the labyrinth with room content markers and navigation cues.',
			icon: 'M9 20l-5.447-2.724A1 1 0 013 16.382V5.618a1 1 0 011.447-.894L9 7m0 13l6-3m-6 3V7m6 10l5.447 2.724A1 1 0 0021 18.382V7.618a1 1 0 00-.553-.894L15 4m0 13V4m0 0L9 7',
		},
	];

	function cycleFeature() {
		currentFeature = (currentFeature + 1) % features.length;
	}

	$effect(() => {
		const interval = setInterval(cycleFeature, 4000);
		return () => clearInterval(interval);
	});
</script>

<svelte:head>
	<title>ProfitOfExile — Lab Farming Companion for Path of Exile 1</title>
	<meta name="description" content="Real-time gem comparison, OCR-powered overlay, and profit analysis for Divine Font farming in Path of Exile." />
	<link rel="preconnect" href="https://fonts.googleapis.com" />
	<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
	<link href="https://fonts.googleapis.com/css2?family=Cinzel:wght@400;700;900&family=Crimson+Pro:ital,wght@0,300;0,400;0,600;1,300&display=swap" rel="stylesheet" />
</svelte:head>

<div class="page">
	<!-- Atmospheric background -->
	<div class="bg-grain"></div>
	<div class="bg-vignette"></div>
	<div class="bg-glow"></div>

	<!-- Hero -->
	<header class="hero">
		<div class="hero-content">
			<img src="/logo-128.png" alt="ProfitOfExile" class="hero-logo" />
		<p class="tagline">Divine Font Farming Companion</p>
			<h1 class="title">
				<span class="title-profit">Profit</span><span class="title-of">Of</span><span class="title-exile">Exile</span>
			</h1>
			<p class="subtitle">
				Real-time gem comparison with OCR detection, in-game overlay,
				and profit analysis for Path of Exile 1 lab farming.
			</p>

			<div class="cta-row">
				<a href="https://github.com/SebRogala/ProfitOfExile/releases/latest/download/ProfitOfExile-setup.exe" class="cta-primary">
					<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="cta-icon"><path d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" /></svg>
					Download for Windows
				</a>
				<a href="https://discord.gg/F7NnSUyBR" class="cta-secondary" target="_blank" rel="noopener">
					<svg viewBox="0 0 24 24" fill="currentColor" class="cta-icon"><path d="M20.317 4.37a19.791 19.791 0 00-4.885-1.515.074.074 0 00-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028c.462-.63.874-1.295 1.226-1.994a.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128 10.2 10.2 0 00.372-.292.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.01c.12.098.246.198.373.292a.077.077 0 01-.006.127 12.299 12.299 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.03z"/></svg>
					Join Discord
				</a>
			</div>
			<p class="platform-note">
				Windows only. Requires Path of Exile 1.
				<a href="https://github.com/SebRogala/ProfitOfExile/releases/latest/download/ProfitOfExile-standalone.exe" class="portable-link">Portable version (no install)</a>
			</p>
		</div>

		<div class="hero-ornament">
			<svg viewBox="0 0 200 2" class="divider"><line x1="0" y1="1" x2="200" y2="1" stroke="url(#gold-fade)" stroke-width="1" /><defs><linearGradient id="gold-fade"><stop offset="0%" stop-color="transparent" /><stop offset="50%" stop-color="#c9aa71" /><stop offset="100%" stop-color="transparent" /></linearGradient></defs></svg>
		</div>
	</header>

	<!-- Features -->
	<section class="features">
		<h2 class="section-heading">How It Works</h2>

		<div class="features-grid">
			{#each features as feature, i}
				<button
					class="feature-card"
					class:active={currentFeature === i}
					onclick={() => { currentFeature = i; }}
				>
					<div class="feature-icon-wrap">
						<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="feature-icon">
							<path d={feature.icon} />
						</svg>
					</div>
					<h3 class="feature-title">{feature.title}</h3>
					<p class="feature-desc">{feature.desc}</p>
				</button>
			{/each}
		</div>
	</section>

	<!-- Setup -->
	<section class="setup">
		<h2 class="section-heading">Quick Setup</h2>

		<div class="steps">
			<div class="step">
				<span class="step-num">1</span>
				<div class="step-content">
					<h3>Download & Install</h3>
					<p>Grab the <a href="https://github.com/SebRogala/ProfitOfExile/releases/latest/download/ProfitOfExile-setup.exe">latest installer</a> and run it — no admin required.</p>
				</div>
			</div>
			<div class="step">
				<span class="step-num">2</span>
				<div class="step-content">
					<h3>Configure OCR Regions</h3>
					<p>Go to Settings &rarr; Game Integration. Configure two red rectangles: one for the <strong>gem tooltip area</strong> (top of screen, where gem names appear on hover), and one for the <strong>font craft panel</strong> (center, where craft options are listed).</p>
					<div class="step-images">
						<figure class="step-figure">
							<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
							<img src="/setup-gem-region.png" alt="Gem tooltip OCR region" class="step-img" onclick={() => { zoomedImg = 'gem'; }} />
							<figcaption>Gem tooltip region <span class="click-hint">(click to enlarge)</span></figcaption>
						</figure>
						<figure class="step-figure">
							<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
							<img src="/setup-font-region.png" alt="Font panel OCR region" class="step-img" onclick={() => { zoomedImg = 'font'; }} />
							<figcaption>Font panel region <span class="click-hint">(click to enlarge)</span></figcaption>
						</figure>
					</div>
				</div>
			</div>
			<div class="step">
				<span class="step-num">3</span>
				<div class="step-content">
					<h3>Position the Overlay</h3>
					<p>Settings &rarr; Overlays &rarr; Comparator Position. Drag the red rectangle where you want the in-game overlay. Bottom-left is recommended — out of the way but visible.</p>
				</div>
			</div>
			<div class="step">
				<span class="step-num">4</span>
				<div class="step-content">
					<h3>Run the Lab</h3>
					<p>The app detects when you enter the 3rd Aspirant's Trial and starts scanning automatically. Hover over Font gems — they appear in the comparator and overlay instantly.</p>
				</div>
			</div>
		</div>
	</section>

	<!-- Dashboard link -->
	<section class="dashboard-link">
		<h2 class="section-heading">Live Dashboard</h2>
		<p class="dashboard-desc">
			The web dashboard shows real-time gem profitability, font EV analysis, and market overview — updated every 30 minutes from poe.ninja data.
		</p>
		<a href="/lab" class="cta-secondary">Open Dashboard</a>
	</section>

	{#if zoomedImg}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="lightbox" onclick={() => { zoomedImg = null; }}>
			<img src={zoomedImg === 'gem' ? '/setup-gem-region.png' : '/setup-font-region.png'} alt="Enlarged view" class="lightbox-img" />
		</div>
	{/if}

	<!-- Footer -->
	<footer class="footer">
		<div class="footer-content">
			<p class="footer-text">
				ProfitOfExile is open source and free.
				Built for the PoE 1 community.
			</p>
			<div class="footer-links">
				<a href="https://discord.gg/F7NnSUyBR" target="_blank" rel="noopener">Discord</a>
				<a href="/lab">Dashboard</a>
				<a href="https://github.com/SebRogala/ProfitOfExile/issues" target="_blank" rel="noopener">Report Issue</a>
			</div>
			<p class="footer-disclaimer">
				Not affiliated with Grinding Gear Games. Path of Exile is a trademark of Grinding Gear Games.
				<br />
				<a href="https://github.com/SebRogala/ProfitOfExile" target="_blank" rel="noopener" class="footer-gh">Source on GitHub</a>
			</p>
		</div>
	</footer>
</div>

<style>
	/* === Typography === */
	:global(body) {
		font-family: 'Crimson Pro', Georgia, serif;
	}

	/* === Page === */
	.page {
		min-height: 100vh;
		background: #0a0a12;
		color: #c8c8d0;
		position: relative;
		overflow-x: hidden;
	}

	/* === Atmospheric layers === */
	.bg-grain {
		position: fixed;
		inset: 0;
		opacity: 0.03;
		background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 256 256' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='noise'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23noise)'/%3E%3C/svg%3E");
		pointer-events: none;
		z-index: 1;
	}

	.bg-vignette {
		position: fixed;
		inset: 0;
		background: radial-gradient(ellipse at center, transparent 40%, #0a0a12 80%);
		pointer-events: none;
		z-index: 1;
	}

	.bg-glow {
		position: fixed;
		top: -200px;
		left: 50%;
		transform: translateX(-50%);
		width: 800px;
		height: 600px;
		background: radial-gradient(ellipse, rgba(201, 170, 113, 0.06) 0%, transparent 70%);
		pointer-events: none;
		z-index: 1;
	}

	/* === Hero === */
	.hero {
		position: relative;
		z-index: 2;
		text-align: center;
		padding: 120px 24px 60px;
		max-width: 720px;
		margin: 0 auto;
	}

	.hero-logo {
		width: 80px;
		height: 80px;
		margin-bottom: 20px;
		opacity: 0;
		animation: fadeUp 0.8s ease forwards;
		filter: drop-shadow(0 0 24px rgba(201, 170, 113, 0.3));
	}

	.tagline {
		font-family: 'Cinzel', serif;
		font-size: 0.8rem;
		font-weight: 400;
		letter-spacing: 0.35em;
		text-transform: uppercase;
		color: #c9aa71;
		margin-bottom: 16px;
		opacity: 0;
		animation: fadeUp 0.8s ease forwards;
	}

	.title {
		font-family: 'Cinzel', serif;
		font-size: clamp(2.5rem, 7vw, 4.5rem);
		font-weight: 900;
		line-height: 1.1;
		margin: 0 0 24px;
		letter-spacing: 0.02em;
		opacity: 0;
		animation: fadeUp 0.8s ease 0.15s forwards;
	}

	.title-profit {
		color: #c9aa71;
	}

	.title-of {
		color: #8a8a9a;
		font-weight: 400;
		font-size: 0.7em;
	}

	.title-exile {
		color: #e0e0e0;
	}

	.subtitle {
		font-size: 1.2rem;
		line-height: 1.7;
		color: #9a9aaa;
		font-weight: 300;
		max-width: 560px;
		margin: 0 auto 40px;
		opacity: 0;
		animation: fadeUp 0.8s ease 0.3s forwards;
	}

	.cta-row {
		display: flex;
		gap: 16px;
		justify-content: center;
		flex-wrap: wrap;
		opacity: 0;
		animation: fadeUp 0.8s ease 0.45s forwards;
	}

	.cta-primary {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		background: linear-gradient(135deg, #c9aa71, #a8894d);
		color: #0a0a12;
		font-family: 'Cinzel', serif;
		font-weight: 700;
		font-size: 0.95rem;
		padding: 14px 28px;
		text-decoration: none;
		letter-spacing: 0.05em;
		transition: transform 0.2s, box-shadow 0.2s;
	}

	.cta-primary:hover {
		transform: translateY(-2px);
		box-shadow: 0 8px 32px rgba(201, 170, 113, 0.25);
	}

	.cta-secondary {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		background: transparent;
		color: #c9aa71;
		font-family: 'Cinzel', serif;
		font-weight: 700;
		font-size: 0.95rem;
		padding: 14px 28px;
		text-decoration: none;
		letter-spacing: 0.05em;
		border: 1px solid rgba(201, 170, 113, 0.3);
		transition: border-color 0.2s, background 0.2s;
	}

	.cta-secondary:hover {
		border-color: rgba(201, 170, 113, 0.6);
		background: rgba(201, 170, 113, 0.05);
	}

	.cta-icon {
		width: 18px;
		height: 18px;
		flex-shrink: 0;
	}

	.platform-note {
		margin-top: 20px;
		font-size: 0.8rem;
		color: #5a5a6a;
		opacity: 0;
		animation: fadeUp 0.8s ease 0.6s forwards;
	}

	.portable-link {
		color: #6a6a7a;
		text-decoration: none;
		border-bottom: 1px solid rgba(106, 106, 122, 0.3);
		margin-left: 8px;
		transition: color 0.2s;
	}

	.portable-link:hover {
		color: #c9aa71;
	}

	.hero-ornament {
		margin-top: 48px;
		opacity: 0;
		animation: fadeUp 0.8s ease 0.7s forwards;
	}

	.divider {
		width: 200px;
		height: 2px;
	}

	/* === Features === */
	.features {
		position: relative;
		z-index: 2;
		max-width: 900px;
		margin: 0 auto;
		padding: 60px 24px 80px;
	}

	.section-heading {
		font-family: 'Cinzel', serif;
		font-size: 1.5rem;
		font-weight: 700;
		color: #e0e0e0;
		text-align: center;
		margin-bottom: 48px;
		letter-spacing: 0.08em;
	}

	.features-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 16px;
	}

	.feature-card {
		all: unset;
		cursor: pointer;
		background: rgba(26, 26, 46, 0.6);
		border: 1px solid rgba(201, 170, 113, 0.08);
		padding: 28px 20px;
		text-align: center;
		transition: border-color 0.3s, background 0.3s, transform 0.2s;
	}

	.feature-card:hover {
		border-color: rgba(201, 170, 113, 0.2);
		background: rgba(26, 26, 46, 0.9);
	}

	.feature-card.active {
		border-color: rgba(201, 170, 113, 0.4);
		background: rgba(201, 170, 113, 0.05);
		transform: translateY(-2px);
	}

	.feature-icon-wrap {
		width: 48px;
		height: 48px;
		margin: 0 auto 16px;
		display: flex;
		align-items: center;
		justify-content: center;
		border: 1px solid rgba(201, 170, 113, 0.2);
		border-radius: 50%;
	}

	.feature-icon {
		width: 24px;
		height: 24px;
		color: #c9aa71;
	}

	.feature-title {
		font-family: 'Cinzel', serif;
		font-size: 0.95rem;
		font-weight: 700;
		color: #e0e0e0;
		margin-bottom: 10px;
		letter-spacing: 0.03em;
	}

	.feature-desc {
		font-size: 0.9rem;
		line-height: 1.6;
		color: #8a8a9a;
		font-weight: 300;
	}

	/* === Setup === */
	.setup {
		position: relative;
		z-index: 2;
		max-width: 640px;
		margin: 0 auto;
		padding: 40px 24px 80px;
	}

	.steps {
		display: flex;
		flex-direction: column;
		gap: 32px;
	}

	.step {
		display: flex;
		gap: 20px;
		align-items: flex-start;
	}

	.step-num {
		font-family: 'Cinzel', serif;
		font-size: 1.8rem;
		font-weight: 900;
		color: rgba(201, 170, 113, 0.3);
		line-height: 1;
		flex-shrink: 0;
		width: 36px;
		text-align: center;
	}

	.step-content h3 {
		font-family: 'Cinzel', serif;
		font-size: 1rem;
		font-weight: 700;
		color: #e0e0e0;
		margin-bottom: 6px;
		letter-spacing: 0.03em;
	}

	.step-content p {
		font-size: 0.95rem;
		line-height: 1.6;
		color: #8a8a9a;
		font-weight: 300;
	}

	.step-images {
		display: flex;
		flex-direction: column;
		gap: 16px;
		margin-top: 16px;
	}

	.step-figure {
		margin: 0;
	}

	.step-img {
		width: 100%;
		border: 1px solid rgba(201, 170, 113, 0.15);
		border-radius: 4px;
		cursor: zoom-in;
		transition: transform 0.2s;
	}

	.lightbox {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.9);
		z-index: 200;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: zoom-out;
	}

	.lightbox-img {
		max-width: 95vw;
		max-height: 90vh;
		border: 1px solid rgba(201, 170, 113, 0.3);
	}

	.step-figure figcaption {
		font-size: 0.8rem;
		color: #5a5a6a;
		margin-top: 6px;
		font-style: italic;
	}

	.click-hint {
		color: #4a4a5a;
		font-size: 0.75rem;
	}

	.step-content a {
		color: #c9aa71;
		text-decoration: none;
		border-bottom: 1px solid rgba(201, 170, 113, 0.3);
		transition: border-color 0.2s;
	}

	.step-content a:hover {
		border-color: #c9aa71;
	}

	/* === Dashboard Link === */
	.dashboard-link {
		position: relative;
		z-index: 2;
		text-align: center;
		padding: 60px 24px 80px;
		border-top: 1px solid rgba(201, 170, 113, 0.08);
		max-width: 640px;
		margin: 0 auto;
	}

	.dashboard-desc {
		font-size: 1rem;
		line-height: 1.7;
		color: #8a8a9a;
		font-weight: 300;
		margin-bottom: 28px;
	}

	/* === Footer === */
	.footer {
		position: relative;
		z-index: 2;
		border-top: 1px solid rgba(255, 255, 255, 0.05);
		padding: 40px 24px;
	}

	.footer-content {
		max-width: 640px;
		margin: 0 auto;
		text-align: center;
	}

	.footer-text {
		font-size: 0.9rem;
		color: #6a6a7a;
		margin-bottom: 16px;
	}

	.footer-links {
		display: flex;
		gap: 24px;
		justify-content: center;
		margin-bottom: 20px;
	}

	.footer-links a {
		color: #8a8a9a;
		text-decoration: none;
		font-size: 0.85rem;
		font-family: 'Cinzel', serif;
		letter-spacing: 0.05em;
		transition: color 0.2s;
	}

	.footer-links a:hover {
		color: #c9aa71;
	}

	.footer-disclaimer {
		font-size: 0.75rem;
		color: #4a4a5a;
		font-style: italic;
	}

	.footer-gh {
		color: #4a4a5a;
		text-decoration: none;
		border-bottom: 1px solid rgba(74, 74, 90, 0.3);
		transition: color 0.2s;
	}

	.footer-gh:hover {
		color: #8a8a9a;
	}

	/* === Animations === */
	@keyframes fadeUp {
		from {
			opacity: 0;
			transform: translateY(16px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
