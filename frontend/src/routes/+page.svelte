<script lang="ts">
	import { PITCH, modules } from '$lib/site-content';

	let currentFeature = $state(0);
	/** The lightbox image: a `static/` path, or null when closed. */
	let zoomedImg = $state<string | null>(null);

	// Icon paths (24×24 outline strokes).
	const ICON_DOC = 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z';
	const ICON_EYE = 'M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z';
	const ICON_COIN = 'M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z';
	const ICON_OVERLAY = 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z';
	const ICON_FLASK = 'M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z';

	/** How It Works — the mechanism every module shares. Module-specific detail belongs in `modules`, not here. */
	const features = [
		{
			title: 'Client.txt Triggers',
			desc: 'Path of Exile logs zone changes and NPC lines to a plain-text file. The app reads it to know when a panel is worth looking at — and looks only then.',
			icon: ICON_DOC,
		},
		{
			title: 'Screen Reading',
			desc: "On a trigger, the app captures the panel's region and reads it with Windows' built-in OCR. Read-only: no game memory, no input, and no screenshot leaves your machine.",
			icon: ICON_EYE,
		},
		{
			title: 'Live Market Data',
			desc: "The public Trade API from your own machine (opt-in), GGG's currency-exchange feed and poe.ninja baselines — real listings, seller concentration, price outliers.",
			icon: ICON_COIN,
		},
		{
			title: 'In-Game Overlays',
			desc: 'Click-through windows show the verdict where you are looking and never block the game. Place each one once in Settings.',
			icon: ICON_OVERLAY,
		},
		{
			title: 'Server-Side Analysis',
			desc: 'The ranking and pricing models run on the ProfitOfExile server and feed both the app and the web dashboard.',
			icon: ICON_FLASK,
		},
	];

	/**
	 * Structured data for search engines and AI crawlers. `featureList` is built
	 * from `modules`, so a new module reaches the crawlers with its entry — and
	 * this is where the module names belong: the meta description has ~155
	 * characters to work with, this has no such budget.
	 */
	const structuredData = {
		'@context': 'https://schema.org',
		'@type': 'SoftwareApplication',
		name: 'ProfitOfExile',
		url: 'https://profitofexile.top/',
		description: PITCH,
		applicationCategory: 'GameApplication',
		applicationSubCategory: 'Game companion overlay',
		operatingSystem: 'Windows 10, Windows 11',
		softwareRequirements: 'Path of Exile 1',
		screenshot: 'https://profitofexile.top/og-card.jpg',
		isAccessibleForFree: true,
		offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
		featureList: modules.map(
			(m) => `${m.name}${m.status === 'beta' ? ' (beta)' : ''}: ${m.tagline}`,
		),
		sameAs: ['https://github.com/SebRogala/ProfitOfExile', 'https://discord.gg/QX53hrv5GP'],
	};

	/**
	 * The OCR-pack walkthrough and Transparency are collapsed by default. The
	 * desktop app links to the former by hash, and a <details> a hash points into
	 * does not open on its own in every browser, so the hash opens it here — on
	 * landing and on change.
	 */
	let ocrPackOpen = $state(false);
	let transparencyOpen = $state(false);
	function openFromHash() {
		if (location.hash === '#ocr-language-pack') ocrPackOpen = true;
		if (location.hash === '#transparency') transparencyOpen = true;
	}
	// Landing from the app's link: no hashchange fires, so check once on mount.
	$effect(openFromHash);

	function cycleFeature() {
		currentFeature = (currentFeature + 1) % features.length;
	}

	function zoomImage(image: string) {
		zoomedImg = image;
	}

	function closeZoom() {
		zoomedImg = null;
	}

	function handleLightboxKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' || e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			closeZoom();
		}
	}

	$effect(() => {
		const interval = setInterval(cycleFeature, 4000);
		return () => clearInterval(interval);
	});
</script>

<svelte:window onhashchange={openFromHash} />

<svelte:head>
	<title>ProfitOfExile — Companion Overlays for Wraeclast</title>
	<!-- 151 characters: Google shows about 155 and writes its own snippet from the
	     body when the tag runs long or reads like a list. -->
	<meta name="description" content={PITCH} />
	<!-- Link previews (Discord, Twitter, Slack). Absolute URLs: scrapers fetch
	     these without a page context, so a relative path resolves to nothing.
	     og-card.jpg is the 1200x630 crop the format wants — swap the file, keep
	     the name and the size. -->
	<meta property="og:type" content="website" />
	<meta property="og:site_name" content="ProfitOfExile" />
	<meta property="og:url" content="https://profitofexile.top/" />
	<meta property="og:title" content="ProfitOfExile — Companion Overlays for Wraeclast" />
	<meta property="og:description" content={PITCH} />
	<meta property="og:image" content="https://profitofexile.top/og-card.jpg" />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:alt" content="Two priced offer boxes from the app over the Temple of Atzoatl sheet in Path of Exile" />
	<meta name="twitter:card" content="summary_large_image" />
	<!-- A literal <script> tag inside <svelte:head> is compiled as component
	     code, so the JSON-LD goes in as markup. The content is our own strings;
	     `<` is escaped anyway so no value can close the tag early. -->
	{@html `<script type="application/ld+json">${JSON.stringify(structuredData).replace(/</g, '\\u003c')}</script>`}
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
		<p class="tagline">Path of Exile Companion</p>
			<h1 class="title">
				<span class="title-profit">Profit</span><span class="title-of">Of</span><span class="title-exile">Exile</span>
			</h1>
			<p class="subtitle">
				Reads the game's own log and screen (where needed), checks the market,
				and shows the verdict in in-game overlays.
			</p>

			<div class="cta-row">
				<a href="https://github.com/SebRogala/ProfitOfExile/releases/latest/download/ProfitOfExile-setup.exe" class="cta-primary">
					<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="cta-icon"><path d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" /></svg>
					Download for Windows
				</a>
				<a href="https://discord.gg/QX53hrv5GP" class="cta-secondary" target="_blank" rel="noopener">
					<svg viewBox="0 0 24 24" fill="currentColor" class="cta-icon"><path d="M20.317 4.37a19.791 19.791 0 00-4.885-1.515.074.074 0 00-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 00-5.487 0 12.64 12.64 0 00-.617-1.25.077.077 0 00-.079-.037A19.736 19.736 0 003.677 4.37a.07.07 0 00-.032.027C.533 9.046-.32 13.58.099 18.057a.082.082 0 00.031.057 19.9 19.9 0 005.993 3.03.078.078 0 00.084-.028c.462-.63.874-1.295 1.226-1.994a.076.076 0 00-.041-.106 13.107 13.107 0 01-1.872-.892.077.077 0 01-.008-.128 10.2 10.2 0 00.372-.292.074.074 0 01.077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 01.078.01c.12.098.246.198.373.292a.077.077 0 01-.006.127 12.299 12.299 0 01-1.873.892.077.077 0 00-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 00.084.028 19.839 19.839 0 006.002-3.03.077.077 0 00.032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 00-.031-.03z"/></svg>
					Join Discord
				</a>
			</div>
			<p class="platform-note">
				Windows only. Requires Path of Exile 1.
				<a href="https://github.com/SebRogala/ProfitOfExile/releases/latest/download/ProfitOfExile-standalone.exe" class="portable-link">Portable version (no install)</a>
				<br />
				Non-English Windows may need the English OCR pack —
				<a href="#ocr-language-pack" class="portable-link" onclick={() => (ocrPackOpen = true)}>instructions</a>
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

	<!-- Modules — one entry per module in `modules` (script). A new module is a new entry plus its screenshots in static/. -->
	<section class="modules" id="modules">
		<h2 class="section-heading">Modules</h2>
		<p class="modules-intro">
			Every module has the same shape: a Client.txt event arms it, it reads one panel while that panel is on
			screen, and it shows its verdict in an overlay you place once. Switch each on or off in the app's sidebar.
		</p>
		<div class="module-list">
			{#each modules as m (m.id)}
				<article class="module" id="module-{m.id}">
					<header class="module-head">
						<h3 class="module-name">{m.name}</h3>
						<span class="module-status" class:beta={m.status === 'beta'}>{m.status === 'beta' ? 'Beta' : 'Available'}</span>
					</header>
					<p class="module-tagline">{m.tagline}</p>
					<dl class="module-facts">
						<dt>Trigger</dt>
						<dd>{m.trigger}</dd>
						<dt>Reads</dt>
						<dd>{m.reads}</dd>
					</dl>
					<ul class="module-gives">
						{#each m.gives as line}
							<li>{line}</li>
						{/each}
					</ul>
					{#if m.setup}
						<p class="module-setup"><strong>Setup:</strong> {m.setup}</p>
					{/if}
					{#if m.screenshots.length}
						<div class="step-images">
							{#each m.screenshots as shot (shot.src)}
								<figure class="step-figure">
									<button class="step-img-button" type="button" onclick={() => zoomImage(shot.src)} aria-label="Enlarge: {shot.alt}">
										<img src={shot.src} alt={shot.alt} class="step-img" loading="lazy" />
									</button>
									<figcaption>{shot.caption} <span class="click-hint">(click to enlarge)</span></figcaption>
								</figure>
							{/each}
						</div>
					{/if}
				</article>
			{/each}
		</div>
		<p class="modules-beta-note">
			Beta modules are switched on per device. Ask on
			<a href="https://discord.gg/QX53hrv5GP" target="_blank" rel="noopener">Discord</a>
			with the device id the app shows under Ctrl+Shift+F11 &rarr; Identify.
		</p>
	</section>

	<!-- Setup — module-agnostic. Anything one module needs by hand goes in its `modules` entry (`setup`), not here. -->
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
					<h3>Check Client.txt</h3>
					<p>Settings &rarr; Game Integration. The app finds <code>Client.txt</code> in the GGG and Steam install folders on its own; if the status says the file is missing, browse to the <code>logs</code> folder of your install.</p>
				</div>
			</div>
			<div class="step">
				<span class="step-num">3</span>
				<div class="step-content">
					<h3>Place the Overlays</h3>
					<p>Settings &rarr; Overlays. Each module has its own group of red rectangles; drag them where you want them over the game and save.</p>
				</div>
			</div>
			<div class="step">
				<span class="step-num">4</span>
				<div class="step-content">
					<h3>Play</h3>
					<p>Switch modules on and off in the sidebar. Each one arms itself from Client.txt, reads its panel while it is on screen, and shows its verdict in the overlay. On a non-English Windows, install the <a href="#ocr-language-pack" onclick={() => (ocrPackOpen = true)}>English OCR pack</a> first.</p>
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
		<div class="step-images">
			<figure class="step-figure">
				<button class="step-img-button" type="button" onclick={() => zoomImage('/dashboard-rankings.webp')} aria-label="Enlarge the Rankings tab">
					<img src="/dashboard-rankings.webp" alt="Rankings tab: gems for the selected variant with tier, price, ROI, stability and market signals, listing counts and a 12-hour sparkline" class="step-img" loading="lazy" />
				</button>
				<figcaption>Rankings — every gem for the variant, with price, ROI, market signals and its 12-hour trend <span class="click-hint">(click to enlarge)</span></figcaption>
			</figure>
			<figure class="step-figure">
				<button class="step-img-button" type="button" onclick={() => zoomImage('/dashboard-font-ev.webp')} aria-label="Enlarge the Font EV tab">
					<img src="/dashboard-font-ev.webp" alt="Font EV tab: chaos per font for red, green and blue against each quality variant, each cell with safe, premium and jackpot odds, over a pool overview by price tier" class="step-img" loading="lazy" />
				</button>
				<figcaption>Font EV — chaos per font by colour and quality, with the safe, premium and jackpot odds behind it <span class="click-hint">(click to enlarge)</span></figcaption>
			</figure>
		</div>
	</section>

	{#if zoomedImg}
		<div
			class="lightbox"
			role="button"
			tabindex="0"
			aria-label="Close enlarged image"
			onclick={closeZoom}
			onkeydown={handleLightboxKeydown}
		>
			<img src={zoomedImg} alt="Enlarged view" class="lightbox-img" />
		</div>
	{/if}

	<!-- Transparency -->
	<section class="transparency">
		<details class="section-details" id="transparency" bind:open={transparencyOpen}>
		<summary class="section-heading section-summary">Transparency & Terms of Service</summary>
		<p class="transparency-intro">
			Transparency is non-negotiable here. This section explains exactly what the desktop app does,
			what it doesn't do, and where the gray areas are.
		</p>

		<div class="transparency-block">
			<h3 class="transparency-subheading">What the App Does</h3>
			<div class="transparency-items">
				<div class="transparency-item">
					<span class="transparency-label">1. Read Client.txt</span>
					<p>Path of Exile writes a plain-text log file called <code>Client.txt</code>. The app watches this file to detect
						game events &mdash; which zone you entered, NPC lines such as Alva's or a recruit's, when the Font of Divine Skill
						craft options appear, when you exit the Labyrinth. This is read-only. The app never writes to or modifies this file.</p>
				</div>
				<div class="transparency-item">
					<span class="transparency-label">2. Screen Capture & OCR</span>
					<p>When a Client.txt event says a panel is on screen, the app captures the region that panel occupies using
						the standard Windows screenshot API and reads it with Windows' built-in text recognition (Windows.Media.Ocr).
						It reads only while the panel is up and within a fixed budget, then stops. Which panel each module reads, and
						what triggers it, is stated in its entry under <a href="#modules">Modules</a>. The automatic trigger is a
						quality-of-life choice &mdash; none of these interactions has a time limit. Client.txt says exactly when the
						panel is on screen, so there's no reason to make you press a hotkey.</p>
					<p><strong>Important privacy detail:</strong> no screenshot or screen capture ever leaves your machine.
						The captured image is processed entirely locally by Windows' built-in OCR engine, then immediately
						discarded. The only data that reaches the server is plain text the app extracted or recorded &mdash; gem names, craft option
						wording, lab run outcomes &mdash; with one exception: the Mercenaries module learns gem icons from the recruit
						window and pools each one through the server as a 24&times;24-pixel signature of the icon's disc, so every
						device recognises it. An icon the pool already knows is matched locally and nothing is sent, so a synced
						device uploads only when it meets art the pool has never seen. Game art only; no screenshot and nothing else
						of your screen ever leaves your machine.</p>
				</div>
				<div class="transparency-item">
					<span class="transparency-label">3. Trade API (opt-in)</span>
					<p>The app can query GGG's public Trade API to look up prices &mdash; gems in the lab, what a captured mercenary is
						going for &mdash; but this is <strong>off by default</strong>. You enable it per module: the comparator's
						"auto-trade" option, the Mercenaries page's trade setting. When enabled,
						the app's rate limiter reads GGG's own rate-limit headers and stays at 65% of the allowed budget with
						additional safety padding.</p>
				</div>
				<div class="transparency-item">
					<span class="transparency-label">4. Price Data from poe.ninja and GGG's Exchange Feed</span>
					<p>Gem and currency prices come from <a href="https://poe.ninja" target="_blank" rel="noopener">poe.ninja</a>
						&mdash; a third-party community aggregator; Currency Exchange plays come from GGG's public currency-exchange feed.
						The ProfitOfExile server fetches both periodically, not the desktop app directly. Nothing scrapes the Path of
						Exile website.</p>
				</div>
				<div class="transparency-item">
					<span class="transparency-label">5. Display an Overlay</span>
					<p>The overlay is a transparent, click-through window. It sits on top of the game but is completely
						non-interactive from the game's perspective &mdash; your cursor and all input pass through it entirely.
						The game does not know the overlay exists.</p>
				</div>
			</div>
		</div>

		<div class="transparency-block">
			<h3 class="transparency-subheading">What the App Never Does</h3>
			<ul class="transparency-never">
				<li><strong>Inject code</strong> into the Path of Exile process</li>
				<li><strong>Read game memory</strong> &mdash; the app has zero access to game internals</li>
				<li><strong>Modify game files</strong> &mdash; not the executable, not data files, nothing</li>
				<li><strong>Send input</strong> to the game &mdash; no clicks, no keystrokes, no mouse movements</li>
				<li><strong>Hook game functions</strong> &mdash; no DLL injection, no API hooking</li>
				<li><strong>Automate gameplay</strong> &mdash; the app does not play the game for you in any way</li>
				<li><strong>Connect to game servers</strong> &mdash; the app only uses the public Trade API (when you opt in), same as the official trade website</li>
			</ul>
		</div>

		<div class="transparency-block">
			<h3 class="transparency-subheading">Why OCR Instead of Clipboard?</h3>
			<p>Tools like Awakened PoE Trade read item data by triggering the game's built-in "Copy Item to Clipboard"
				feature. This works because those items <strong>exist</strong> in the game world &mdash; they have an
				internal ID and full metadata that the game can serialize to text.</p>
			<p>Font of Divine Skill craft options are different. The gems shown as choices are <strong>draft items</strong>
				&mdash; they don't exist yet. They have no item ID, no metadata the game can copy. There is nothing to put
				on your clipboard. The same applies to Grand Heist reward choices and similar "pick one" mechanics.</p>
			<p>OCR is the only way to read what's on screen when the game itself has no copy mechanism for it.</p>
		</div>

		<div class="transparency-block">
			<h3 class="transparency-subheading">Where the App Sits Among Community Tools</h3>
			<p>Not all tools work the same way. Here's an honest comparison:</p>
			<div class="transparency-table-wrap">
				<table class="transparency-table">
					<thead>
						<tr>
							<th>Tool</th>
							<th>How It Reads Game Data</th>
							<th>When It Activates</th>
						</tr>
					</thead>
					<tbody>
						<tr>
							<td><a href="https://github.com/SnosMe/awakened-poe-trade" target="_blank" rel="noopener">Awakened PoE Trade</a></td>
							<td>Sends Ctrl+C to game on hotkey, reads clipboard</td>
							<td>Hotkey (default Ctrl+D)</td>
						</tr>
						<tr>
							<td><a href="https://github.com/Lailloken/Exile-UI" target="_blank" rel="noopener">Exile-UI</a></td>
							<td>Reads screen pixels via OCR</td>
							<td>Manual hotkey</td>
						</tr>
						<tr>
							<td><a href="https://github.com/Morph21/MercuryTrade-Community-Fork" target="_blank" rel="noopener">MercuryTrade</a></td>
							<td>Duplicates a screen region to display elsewhere</td>
							<td>Set once, always mirrors</td>
						</tr>
						<tr>
							<td><a href="https://github.com/yznpku/LabCompass" target="_blank" rel="noopener">LabCompass</a></td>
							<td>Reads Client.txt for zone changes</td>
							<td>Automatic &mdash; overlay updates on room transition</td>
						</tr>
						<tr>
							<td><a href="https://github.com/dermow/TraXile" target="_blank" rel="noopener">TraXile</a></td>
							<td>Reads Client.txt for map/zone events</td>
							<td>Automatic &mdash; overlay reacts to game log</td>
						</tr>
						<tr class="transparency-table-highlight">
							<td>ProfitOfExile</td>
							<td>Reads Client.txt + screen pixels via OCR</td>
							<td>Automatic &mdash; triggered by game log event</td>
						</tr>
					</tbody>
				</table>
			</div>
			<p>Each technique the app uses exists individually across established community tools: auto-trigger from Client.txt
				(LabCompass, TraXile), screen reading (MercuryTrade, Exile-UI). The app combines them.</p>
		</div>

		<div class="transparency-block">
			<h3 class="transparency-subheading">GGG's Known Stance on Community Tools</h3>
			<p>GGG has given explicit guidance on tool interactions. In a
				<a href="https://www.pathofexile.com/forum/view-thread/1675115" target="_blank" rel="noopener">forum post</a>,
				the policy is clear: <strong>one server-side action per one keypress.</strong> A single key can send
				<code>/hideout</code> or a trade whisper &mdash; but not both. This is why overlay tools with trade response
				buttons or party management are considered fine.</p>
			<p>ProfitOfExile doesn't send any input to the game at all. No chat commands, no clicks, no keystrokes &mdash;
				nothing. The app is entirely below the threshold of what GGG has explicitly allowed for community tools.</p>
		</div>

		<div class="transparency-block transparency-gray">
			<h3 class="transparency-subheading">The Gray Area &mdash; Honest Assessment</h3>
			<p>GGG's Terms of Use prohibit <em>"automated software or bots in relation to your access or use of the
				Services."</em></p>
			<p><strong>The author's position:</strong></p>
			<ul class="transparency-position">
				<li>The app is <strong>passive</strong> &mdash; it reads information and displays it. It never acts on your behalf.</li>
				<li>It does not automate gameplay. You still make every decision and every click yourself.</li>
				<li>It sends <strong>zero input</strong> to the game &mdash; well below GGG's "one action per one keypress"
					guideline that other tools actively use.</li>
				<li>Client.txt reading is an established community practice used by dozens of tools.</li>
				<li>Screen OCR is used by other community tools (e.g. Exile-UI, MercuryTrade), though typically triggered manually.</li>
			</ul>
			<p><strong>The honest truth:</strong></p>
			<ul class="transparency-position">
				<li><strong>GGG has never explicitly approved any local third-party tool.</strong> Not Awakened PoE Trade,
					not Exile-UI, not Path of Building, not LabCompass. Community tools exist in a space of implicit tolerance.</li>
				<li><strong>No bans have been reported</strong> for using OCR-based overlay tools. This is weak but real evidence.</li>
				<li><strong>Auto-triggered OCR is the part closest to the line.</strong> I believe it falls on the right side
					because it's read-only information display, not automation &mdash; but I cannot guarantee GGG sees it the
					same way.</li>
			</ul>
			<p class="transparency-promise">If GGG ever communicates that this approach crosses a line, I will change it
				immediately. I have no interest in putting your account at risk.</p>
		</div>

		<div class="transparency-block">
			<h3 class="transparency-subheading">You Are in Control</h3>
			<ul class="transparency-position">
				<li>You can see when OCR is active (the scan indicator is visible on the overlay)</li>
				<li>You can adjust or disable features in settings</li>
				<li>The app is <a href="https://github.com/SebRogala/ProfitOfExile" target="_blank" rel="noopener">open source</a>
					&mdash; you can inspect exactly what it does</li>
			</ul>
		</div>
		</details>
	</section>

	<!-- English OCR pack — the desktop app links here (Settings → OCR Regions warning) with #ocr-language-pack; keep the id stable. -->
	<section class="setup ocr-pack">
		<details class="section-details" id="ocr-language-pack" bind:open={ocrPackOpen}>
		<summary class="section-heading section-summary">English OCR Language Pack</summary>
		<p class="ocr-pack-intro">
			The app reads gem names with the OCR built into Windows, pinned to <strong>English (United States)</strong> because Path of Exile draws its UI in English whatever language Windows runs in. Without that pack Windows falls back to your own language and gem names come out garbled — the app then shows a red warning at the top of Settings &rarr; OCR Regions. Installing the pack takes a minute and does not change your Windows display language.
		</p>

		<div class="steps">
			<div class="step">
				<span class="step-num">1</span>
				<div class="step-content">
					<h3>Open the language settings</h3>
					<p>Windows Settings &rarr; <strong>Time &amp; language</strong> &rarr; <strong>Language &amp; region</strong>. On Windows 10 the page is called <strong>Language</strong>.</p>
				</div>
			</div>
			<div class="step">
				<span class="step-num">2</span>
				<div class="step-content">
					<h3>Add English (United States)</h3>
					<p>Under <strong>Preferred languages</strong> click <strong>Add a language</strong>, search for <strong>English (United States)</strong> and click <strong>Next</strong>. If it is already in the list, open its <strong>&hellip;</strong> menu &rarr; <strong>Language options</strong> instead and continue with step 3.</p>
					<div class="step-images">
						<figure class="step-figure">
							<button class="step-img-button" type="button" onclick={() => zoomImage('/setup-ocr-pack-language.png')} aria-label="Enlarge the Add a language dialog">
								<img src="/setup-ocr-pack-language.png" alt="Add a language dialog with English (United States) found" class="step-img" />
							</button>
							<figcaption>Add a language &rarr; English (United States) <span class="click-hint">(click to enlarge)</span></figcaption>
						</figure>
					</div>
				</div>
			</div>
			<div class="step">
				<span class="step-num">3</span>
				<div class="step-content">
					<h3>Install Optical character recognition</h3>
					<p>On the <strong>Install language features</strong> page, <strong>Optical character recognition</strong> is listed under <strong>Required language features</strong> and installs with the language on its own — if it is missing there, that Windows build has no OCR for the language. Untick every <strong>Optional language feature</strong>, leave <strong>Set as my Windows display language</strong> unticked, then click <strong>Install</strong>. For a language that was already listed, the feature sits under <strong>Language features</strong> on its Language options page with a download button beside it.</p>
					<div class="step-images">
						<figure class="step-figure">
							<button class="step-img-button" type="button" onclick={() => zoomImage('/setup-ocr-pack-features.png')} aria-label="Enlarge the Install language features page">
								<img src="/setup-ocr-pack-features.png" alt="Install language features page, Optical character recognition under Required language features" class="step-img" />
							</button>
							<figcaption>Optical character recognition under Required language features <span class="click-hint">(click to enlarge)</span></figcaption>
						</figure>
					</div>
				</div>
			</div>
			<div class="step">
				<span class="step-num">4</span>
				<div class="step-content">
					<h3>Restart Profit of Exile</h3>
					<p>The app picks its recognizer once per session, so close and reopen it after the download finishes. With the pack in place the warning in Settings &rarr; OCR Regions is gone.</p>
				</div>
			</div>
			<div class="step">
				<span class="step-num step-num-text">PS</span>
				<div class="step-content">
					<h3>PowerShell instead of Settings</h3>
					<p>Open <strong>Windows PowerShell</strong> as administrator (not PowerShell 7) and run the two lines; the first is the basic pack the OCR pack depends on. The download can take a few minutes.</p>
					<pre class="ocr-pack-code"><code>Get-WindowsCapability -Online | Where-Object &#123; $_.Name -Like 'Language.Basic*en-US*' &#125; | Add-WindowsCapability -Online
Get-WindowsCapability -Online | Where-Object &#123; $_.Name -Like 'Language.OCR*en-US*' &#125; | Add-WindowsCapability -Online</code></pre>
					<p>To check, run the line below: <code>State : Installed</code> means the pack is in place.</p>
					<pre class="ocr-pack-code"><code>Get-WindowsCapability -Online | Where-Object &#123; $_.Name -Like 'Language.OCR*en-US*' &#125;</code></pre>
				</div>
			</div>
		</div>
		</details>
	</section>

	<!-- Credits -->
	<section class="credits">
		<h2 class="section-heading">Standing on the Shoulders of Giants</h2>
		<div class="credits-list">
			<div class="credit">
				<a href="https://www.poelab.com/" target="_blank" rel="noopener">poelab.com</a>
				<span class="credit-desc">Without their daily lab layouts, our lab map overlays wouldn't exist</span>
			</div>
			<div class="credit">
				<a href="https://github.com/yznpku/LabCompass" target="_blank" rel="noopener">LabCompass</a>
				<span class="credit-desc">The original lab compass — our navigation system is built on their work</span>
			</div>
			<div class="credit">
				<a href="https://poe.ninja/" target="_blank" rel="noopener">poe.ninja</a>
				<span class="credit-desc">Powers our entire analysis engine with reliable price data</span>
			</div>
		</div>
		<p class="credits-note">
			Gem prices are also periodically fetched from the GGG trade API. If the tool gains more traction, an official API key will be requested for dedicated price collection.
		</p>
	</section>

	<!-- Footer -->
	<footer class="footer">
		<div class="footer-content">
			<p class="footer-text">
				ProfitOfExile is open source and free.
				Built for the PoE 1 community.
			</p>
			<div class="footer-links">
				<a href="https://discord.gg/QX53hrv5GP" target="_blank" rel="noopener">Discord</a>
				<a href="/lab">Dashboard</a>
				<a href="https://discord.com/channels/1489388543859232899/1489599916278808748" target="_blank" rel="noopener">Report Issue</a>
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
		/* One knob per axis, so the page scales from three numbers.
		   `--fs` rather than a root font-size: `rem` is app-global and would
		   drag the dashboard along with the landing page. */
		--content: 1100px;
		/* Only the centred intro paragraphs; body copy fills its shell. */
		--measure: 92ch;
		--fs: 1.2rem;
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
		max-width: 880px;
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
		font-size: calc(0.9 * var(--fs));
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
		font-size: calc(1.3 * var(--fs));
		line-height: 1.7;
		color: #b0b0be;
		font-weight: 300;
		max-width: 660px;
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
		background: linear-gradient(135deg, #d4b87a, #b89855);
		color: #0a0a12;
		font-family: 'Cinzel', serif;
		font-weight: 700;
		font-size: calc(1.05 * var(--fs));
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
		color: #d4b87a;
		font-family: 'Cinzel', serif;
		font-weight: 700;
		font-size: calc(1.05 * var(--fs));
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
		font-size: calc(0.9 * var(--fs));
		color: #7a7a8a;
		opacity: 0;
		animation: fadeUp 0.8s ease 0.6s forwards;
	}

	.portable-link {
		color: #8a8a9a;
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
		max-width: var(--content);
		margin: 0 auto;
		padding: 60px 24px 80px;
	}

	.section-heading {
		font-family: 'Cinzel', serif;
		font-size: calc(1.7 * var(--fs));
		font-weight: 700;
		color: #e0e0e0;
		text-align: center;
		margin-bottom: 48px;
		letter-spacing: 0.08em;
	}

	.features-grid {
		display: flex;
		flex-wrap: wrap;
		gap: 16px;
		justify-content: center;
	}

	.feature-card {
		all: unset;
		cursor: pointer;
		width: calc(33.333% - 11px);
		min-width: 200px;
		box-sizing: border-box;
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
		font-size: calc(1.05 * var(--fs));
		font-weight: 700;
		color: #e0e0e0;
		margin-bottom: 10px;
		letter-spacing: 0.03em;
	}

	.feature-desc {
		font-size: calc(1 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
		font-weight: 300;
	}

	/* === Modules === */
	.modules {
		position: relative;
		z-index: 2;
		max-width: var(--content);
		margin: 0 auto;
		padding: 60px 24px 80px;
		border-top: 1px solid rgba(201, 170, 113, 0.08);
	}

	.modules-intro {
		font-size: calc(1.05 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
		font-weight: 300;
		text-align: center;
		max-width: var(--measure);
		margin: -24px auto 40px;
	}


	.module-list {
		display: flex;
		flex-direction: column;
		gap: 32px;
	}

	.module {
		background: rgba(26, 26, 46, 0.6);
		border: 1px solid rgba(201, 170, 113, 0.08);
		padding: 28px 24px;
	}

	.module-head {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		gap: 12px;
		margin-bottom: 6px;
	}

	.module-name {
		font-family: 'Cinzel', serif;
		font-size: calc(1.25 * var(--fs));
		font-weight: 700;
		color: #e0e0e0;
		letter-spacing: 0.03em;
	}

	.module-status {
		font-size: calc(0.7 * var(--fs));
		letter-spacing: 0.1em;
		text-transform: uppercase;
		padding: 2px 8px;
		border: 1px solid rgba(201, 170, 113, 0.3);
		border-radius: 2px;
		color: #c9aa71;
	}

	.module-status.beta {
		border-color: rgba(94, 234, 212, 0.4);
		color: #5eead4;
	}

	.module-tagline {
		font-size: calc(1.05 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
		font-weight: 300;
		margin-bottom: 16px;
	}

	.module-facts {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 6px 16px;
		margin: 0 0 16px;
		font-size: calc(0.95 * var(--fs));
		line-height: 1.5;
	}

	.module-facts dt {
		color: #c9aa71;
		font-size: calc(0.75 * var(--fs));
		letter-spacing: 0.1em;
		text-transform: uppercase;
		padding-top: 3px;
	}

	.module-facts dd {
		margin: 0;
		color: #a0a0b0;
		font-weight: 300;
	}

	.module-gives {
		list-style: none;
		padding: 0;
		margin: 0 0 16px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.module-gives li {
		position: relative;
		padding-left: 18px;
		font-size: calc(1 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
		font-weight: 300;
	}

	.module-gives li::before {
		content: '\25C6';
		position: absolute;
		left: 0;
		top: 0;
		font-size: calc(0.55 * var(--fs));
		/* The marker's line box has to match the li's first line, or its baseline
		   — and with it the diamond — rides above the text. `1.6rem` was fixed
		   while the line it sits on scales with --fs. */
		line-height: calc(1.6 * var(--fs));
		color: rgba(201, 170, 113, 0.5);
	}

	.module-setup {
		font-size: calc(0.95 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
		font-weight: 300;
	}

	.module-setup strong {
		color: #e0e0e0;
		font-weight: 500;
	}

	.modules-beta-note {
		margin-top: 32px;
		text-align: center;
		font-size: calc(0.9 * var(--fs));
		line-height: 1.6;
		color: #7a7a8a;
	}

	.modules-beta-note a {
		color: #c9aa71;
		text-decoration: none;
		border-bottom: 1px solid rgba(201, 170, 113, 0.3);
	}

	/* === Setup === */
	.setup {
		position: relative;
		z-index: 2;
		max-width: var(--content);
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
		font-size: calc(1.8 * var(--fs));
		font-weight: 900;
		color: rgba(201, 170, 113, 0.3);
		line-height: 1;
		flex-shrink: 0;
		width: 36px;
		text-align: center;
	}

	.step-content h3 {
		font-family: 'Cinzel', serif;
		font-size: calc(1.1 * var(--fs));
		font-weight: 700;
		color: #e0e0e0;
		margin-bottom: 6px;
		letter-spacing: 0.03em;
	}

	.step-content p {
		font-size: calc(1.05 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
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

	.step-img-button {
		all: unset;
		display: block;
		width: 100%;
		cursor: zoom-in;
	}

	.step-img-button:focus-visible {
		outline: 2px solid #c9aa71;
		outline-offset: 3px;
	}

	.step-img {
		width: 100%;
		border: 1px solid rgba(201, 170, 113, 0.15);
		border-radius: 4px;
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
		font-size: calc(0.9 * var(--fs));
		color: #8a8a9a;
		margin-top: 6px;
		font-style: italic;
	}

	.click-hint {
		color: #6a6a7a;
		font-size: calc(0.8 * var(--fs));
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

	.ocr-pack {
		padding: 40px 24px 60px;
		border-top: 1px solid rgba(201, 170, 113, 0.08);
	}

	.section-summary {
		cursor: pointer;
		list-style: none;
		margin-bottom: 0;
		user-select: none;
	}

	.section-summary::-webkit-details-marker {
		display: none;
	}

	.section-summary::after {
		content: '\25BE';
		display: inline-block;
		margin-left: 12px;
		color: rgba(201, 170, 113, 0.6);
		transition: transform 0.2s;
	}

	.section-details[open] > .section-summary {
		margin-bottom: 40px;
	}

	.section-details[open] > .section-summary::after {
		transform: rotate(180deg);
	}

	.step-num-text {
		font-size: calc(1 * var(--fs));
		padding-top: 8px;
	}

	.ocr-pack-intro {
		font-size: calc(1.05 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
		font-weight: 300;
		margin-bottom: 32px;
	}

	.ocr-pack-intro strong,
	.ocr-pack .step-content strong {
		color: #e0e0e0;
		font-weight: 500;
	}

	.ocr-pack-code {
		margin: 12px 0;
		padding: 10px 12px;
		background: rgba(201, 170, 113, 0.08);
		border: 1px solid rgba(201, 170, 113, 0.15);
		border-radius: 4px;
		font-family: monospace;
		font-size: calc(0.85 * var(--fs));
		line-height: 1.5;
		color: #d4b87a;
		overflow-x: auto;
		white-space: pre;
	}

	.step-content code {
		font-family: monospace;
		color: #d4b87a;
	}

	/* === Dashboard Link === */
	.dashboard-link {
		position: relative;
		z-index: 2;
		text-align: center;
		padding: 60px 24px 80px;
		border-top: 1px solid rgba(201, 170, 113, 0.08);
		/* Matches the Modules section: the dashboard shots are ~1490 px wide and
		   unreadable in the 640 px column this section used before they existed. */
		max-width: var(--content);
		margin: 0 auto;
	}

	/* The screenshots sit under the CTA, so they need more air than the 16 px
	   `.step-images` gives them inside a module. */
	.dashboard-link .step-images {
		margin-top: 36px;
	}

	.dashboard-desc {
		font-size: calc(1.1 * var(--fs));
		line-height: 1.7;
		color: #a0a0b0;
		font-weight: 300;
		max-width: var(--measure);
		margin: 0 auto 28px;
	}

	/* === Credits === */
	.credits {
		position: relative;
		z-index: 2;
		max-width: 820px;
		margin: 0 auto;
		padding: 60px 24px 60px;
		border-top: 1px solid rgba(201, 170, 113, 0.08);
		text-align: center;
	}

	.credits-list {
		display: flex;
		flex-direction: column;
		gap: 14px;
		margin-bottom: 24px;
	}

	.credit {
		display: flex;
		align-items: baseline;
		justify-content: center;
		gap: 12px;
	}

	.credit a {
		color: #d4b87a;
		text-decoration: none;
		font-family: 'Cinzel', serif;
		font-size: calc(1 * var(--fs));
		font-weight: 700;
		letter-spacing: 0.03em;
		border-bottom: 1px solid rgba(201, 170, 113, 0.2);
		transition: border-color 0.2s;
	}

	.credit a:hover {
		border-color: #c9aa71;
	}

	.credit-desc {
		font-size: calc(0.95 * var(--fs));
		color: #8a8a9a;
		font-weight: 300;
	}

	.credits-note {
		font-size: calc(0.9 * var(--fs));
		color: #7a7a8a;
		line-height: 1.6;
		font-style: italic;
	}

	/* === Footer === */
	.footer {
		position: relative;
		z-index: 2;
		border-top: 1px solid rgba(255, 255, 255, 0.05);
		padding: 40px 24px;
	}

	.footer-content {
		max-width: 820px;
		margin: 0 auto;
		text-align: center;
	}

	.footer-text {
		font-size: calc(0.95 * var(--fs));
		color: #8a8a9a;
		margin-bottom: 16px;
	}

	.footer-links {
		display: flex;
		gap: 24px;
		justify-content: center;
		margin-bottom: 20px;
	}

	.footer-links a {
		color: #9a9aaa;
		text-decoration: none;
		font-size: calc(0.95 * var(--fs));
		font-family: 'Cinzel', serif;
		letter-spacing: 0.05em;
		transition: color 0.2s;
	}

	.footer-links a:hover {
		color: #c9aa71;
	}

	.footer-disclaimer {
		font-size: calc(0.75 * var(--fs));
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

	/* === Transparency === */
	.transparency {
		position: relative;
		z-index: 2;
		max-width: var(--content);
		margin: 0 auto;
		padding: 60px 24px 80px;
		border-top: 1px solid rgba(201, 170, 113, 0.08);
	}

	.transparency-intro {
		font-size: calc(1.15 * var(--fs));
		line-height: 1.7;
		color: #b0b0be;
		font-weight: 300;
		text-align: center;
		max-width: var(--measure);
		margin: 0 auto 48px;
	}

	.transparency-block {
		margin-bottom: 36px;
	}

	.transparency-subheading {
		font-family: 'Cinzel', serif;
		font-size: calc(1.15 * var(--fs));
		font-weight: 700;
		color: #d4b87a;
		letter-spacing: 0.04em;
		margin-bottom: 16px;
	}

	.transparency-block p {
		font-size: calc(1.05 * var(--fs));
		line-height: 1.7;
		color: #a0a0b0;
		font-weight: 300;
		margin-bottom: 12px;
	}

	.transparency-block a {
		color: #c9aa71;
		text-decoration: none;
		border-bottom: 1px solid rgba(201, 170, 113, 0.3);
		transition: border-color 0.2s;
	}

	.transparency-block a:hover {
		border-color: #c9aa71;
	}

	.transparency-block code {
		font-family: monospace;
		background: rgba(201, 170, 113, 0.08);
		padding: 2px 6px;
		font-size: 0.95em;
		color: #d4b87a;
	}

	.transparency-items {
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	.transparency-item {
		padding-left: 16px;
		border-left: 2px solid rgba(201, 170, 113, 0.15);
	}

	.transparency-label {
		font-family: 'Cinzel', serif;
		font-size: calc(0.95 * var(--fs));
		font-weight: 700;
		color: #e0e0e0;
		letter-spacing: 0.02em;
		display: block;
		margin-bottom: 6px;
	}

	.transparency-item p {
		font-size: calc(1 * var(--fs));
		margin-bottom: 0;
	}

	.transparency-never {
		list-style: none;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.transparency-never li {
		font-size: calc(1.05 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
		font-weight: 300;
		padding-left: 24px;
		position: relative;
	}

	.transparency-never li::before {
		content: '\2717';
		position: absolute;
		left: 0;
		color: #8b4040;
		font-weight: 700;
	}

	.transparency-never li strong {
		color: #c8c8d0;
	}

	.transparency-table-wrap {
		overflow-x: auto;
		margin: 16px 0;
	}

	.transparency-table {
		width: 100%;
		border-collapse: collapse;
		font-size: calc(0.95 * var(--fs));
	}

	.transparency-table th {
		font-family: 'Cinzel', serif;
		font-size: calc(0.85 * var(--fs));
		font-weight: 700;
		color: #c9aa71;
		letter-spacing: 0.06em;
		text-align: left;
		padding: 10px 12px;
		border-bottom: 1px solid rgba(201, 170, 113, 0.2);
	}

	.transparency-table td {
		padding: 10px 12px;
		color: #a0a0b0;
		font-weight: 300;
		border-bottom: 1px solid rgba(255, 255, 255, 0.04);
	}

	.transparency-table a {
		color: #c9aa71;
		text-decoration: none;
		border-bottom: 1px solid rgba(201, 170, 113, 0.3);
		transition: border-color 0.2s;
	}

	.transparency-table a:hover {
		border-color: #c9aa71;
	}

	.transparency-table tbody tr:last-child td {
		border-bottom: none;
	}

	.transparency-table-highlight {
		background: rgba(201, 170, 113, 0.06);
	}

	.transparency-table-highlight td {
		color: #e0e0e0;
		font-weight: 400;
	}

	.transparency-gray {
		background: rgba(26, 26, 46, 0.6);
		border: 1px solid rgba(201, 170, 113, 0.1);
		padding: 28px 24px;
	}

	.transparency-position {
		list-style: none;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-bottom: 16px;
	}

	.transparency-position li {
		font-size: calc(1.05 * var(--fs));
		line-height: 1.6;
		color: #a0a0b0;
		font-weight: 300;
		padding-left: 20px;
		position: relative;
	}

	.transparency-position li::before {
		content: '\2022';
		position: absolute;
		left: 0;
		color: #c9aa71;
	}

	.transparency-position li strong {
		color: #c8c8d0;
	}

	.transparency-promise {
		font-style: italic;
		color: #c9aa71 !important;
		font-weight: 400 !important;
		margin-bottom: 0 !important;
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
