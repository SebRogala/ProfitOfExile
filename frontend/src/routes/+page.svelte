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
			desc: 'Compass and path strip overlays show your route through the labyrinth with room content markers, navigation cues, and lab room progress.',
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
					<h3>Position the Overlays</h3>
					<p>Settings &rarr; Overlays to configure three in-game overlays: the <strong>Comparator</strong> (gem comparison with trade data), the <strong>Path Strip</strong> (lab room progress), and the <strong>Compass</strong> (room map with content markers and navigation). Drag each red rectangle where you want it.</p>
					<div class="step-images">
						<figure class="step-figure">
							<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
							<img src="/overlay-labmap.png" alt="Lab map path strip overlay" class="step-img" onclick={() => { zoomedImg = 'labmap'; }} />
							<figcaption>Path strip overlay — lab progress with room contents <span class="click-hint">(click to enlarge)</span></figcaption>
						</figure>
					</div>
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
			<img src={zoomedImg === 'gem' ? '/setup-gem-region.png' : zoomedImg === 'font' ? '/setup-font-region.png' : '/overlay-labmap.png'} alt="Enlarged view" class="lightbox-img" />
		</div>
	{/if}

	<!-- Transparency -->
	<section class="transparency" id="transparency">
		<h2 class="section-heading">Transparency & Terms of Service</h2>
		<p class="transparency-intro">
			We believe transparency is non-negotiable. This section explains exactly what the desktop app does,
			what it doesn't do, and where the gray areas are.
		</p>

		<div class="transparency-block">
			<h3 class="transparency-subheading">What We Do</h3>
			<div class="transparency-items">
				<div class="transparency-item">
					<span class="transparency-label">1. Read Client.txt</span>
					<p>Path of Exile writes a plain-text log file called <code>Client.txt</code>. We watch this file to detect
						game events &mdash; which zone you entered, when the Font of Divine Skill craft options appear, when you
						exit the Labyrinth. This is read-only. We never write to or modify this file.</p>
				</div>
				<div class="transparency-item">
					<span class="transparency-label">2. Screen Capture & OCR</span>
					<p>When the Font craft options appear (detected via Client.txt), the app automatically captures a small region
						of your screen (550&times;75 pixels by default) using the standard Windows screenshot API. It then runs
						Windows' built-in text recognition (Windows.Media.Ocr) to read gem names. This runs every 250ms for a
						maximum of 45 seconds, then stops. The automatic trigger is a quality-of-life choice &mdash; there is no
						time limit on the Font interaction. We trigger it automatically because Client.txt tells us exactly when
						the craft options are on screen, so there's no reason to make you press a hotkey.</p>
					<p><strong>Important privacy detail:</strong> no screenshot or screen capture ever leaves your machine.
						The captured image is processed entirely locally by Windows' built-in OCR engine, then immediately
						discarded. The only data sent to the server is the extracted text &mdash; gem names and font crafting
						option wording. Plain text, nothing else.</p>
				</div>
				<div class="transparency-item">
					<span class="transparency-label">3. Trade API (opt-in)</span>
					<p>The app can query GGG's public Trade API to look up gem prices &mdash; but this is <strong>off by
						default</strong>. You enable it manually via the "auto-trade" option in the comparator. When enabled,
						our rate limiter reads GGG's own rate-limit headers and stays at 65% of the allowed budget with
						additional safety padding.</p>
				</div>
				<div class="transparency-item">
					<span class="transparency-label">4. Price Data from poe.ninja</span>
					<p>Gem and currency prices come from <a href="https://poe.ninja" target="_blank" rel="noopener">poe.ninja</a>
						&mdash; a third-party community aggregator. Our server fetches this data periodically, not the desktop
						app directly. We do not scrape the Path of Exile website.</p>
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
			<h3 class="transparency-subheading">What We Never Do</h3>
			<ul class="transparency-never">
				<li><strong>Inject code</strong> into the Path of Exile process</li>
				<li><strong>Read game memory</strong> &mdash; we have zero access to game internals</li>
				<li><strong>Modify game files</strong> &mdash; not the executable, not data files, nothing</li>
				<li><strong>Send input</strong> to the game &mdash; no clicks, no keystrokes, no mouse movements</li>
				<li><strong>Hook game functions</strong> &mdash; no DLL injection, no API hooking</li>
				<li><strong>Automate gameplay</strong> &mdash; the app does not play the game for you in any way</li>
				<li><strong>Connect to game servers</strong> &mdash; we only use the public Trade API (when you opt in), same as the official trade website</li>
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
			<h3 class="transparency-subheading">Where We Sit Among Community Tools</h3>
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
			<p>Each technique we use exists individually across established community tools: auto-trigger from Client.txt
				(LabCompass, TraXile), screen reading (MercuryTrade, Exile-UI). We combine them.</p>
		</div>

		<div class="transparency-block">
			<h3 class="transparency-subheading">GGG's Known Stance on Community Tools</h3>
			<p>GGG has given explicit guidance on tool interactions. In a
				<a href="https://www.pathofexile.com/forum/view-thread/1675115" target="_blank" rel="noopener">forum post</a>,
				the policy is clear: <strong>one server-side action per one keypress.</strong> A single key can send
				<code>/hideout</code> or a trade whisper &mdash; but not both. This is why overlay tools with trade response
				buttons or party management are considered fine.</p>
			<p>ProfitOfExile doesn't send any input to the game at all. No chat commands, no clicks, no keystrokes &mdash;
				nothing. We are entirely below the threshold of what GGG has explicitly allowed for community tools.</p>
		</div>

		<div class="transparency-block transparency-gray">
			<h3 class="transparency-subheading">The Gray Area &mdash; Honest Assessment</h3>
			<p>GGG's Terms of Use prohibit <em>"automated software or bots in relation to your access or use of the
				Services."</em></p>
			<p><strong>Our position:</strong></p>
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
				<li><strong>Auto-triggered OCR is the part closest to the line.</strong> We believe it falls on the right side
					because it's read-only information display, not automation &mdash; but we cannot guarantee GGG sees it the
					same way.</li>
			</ul>
			<p class="transparency-promise">If GGG ever communicates that this approach crosses a line, we will change it
				immediately. We have no interest in putting your account at risk.</p>
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
				<a href="https://discord.gg/F7NnSUyBR" target="_blank" rel="noopener">Discord</a>
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
		font-size: 0.9rem;
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
		font-size: 1.3rem;
		line-height: 1.7;
		color: #b0b0be;
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
		background: linear-gradient(135deg, #d4b87a, #b89855);
		color: #0a0a12;
		font-family: 'Cinzel', serif;
		font-weight: 700;
		font-size: 1.05rem;
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
		font-size: 1.05rem;
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
		font-size: 0.9rem;
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
		max-width: 900px;
		margin: 0 auto;
		padding: 60px 24px 80px;
	}

	.section-heading {
		font-family: 'Cinzel', serif;
		font-size: 1.7rem;
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
		font-size: 1.05rem;
		font-weight: 700;
		color: #e0e0e0;
		margin-bottom: 10px;
		letter-spacing: 0.03em;
	}

	.feature-desc {
		font-size: 1rem;
		line-height: 1.6;
		color: #a0a0b0;
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
		font-size: 1.1rem;
		font-weight: 700;
		color: #e0e0e0;
		margin-bottom: 6px;
		letter-spacing: 0.03em;
	}

	.step-content p {
		font-size: 1.05rem;
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
		font-size: 1.1rem;
		line-height: 1.7;
		color: #a0a0b0;
		font-weight: 300;
		margin-bottom: 28px;
	}

	/* === Credits === */
	.credits {
		position: relative;
		z-index: 2;
		max-width: 640px;
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
		font-size: 1rem;
		font-weight: 700;
		letter-spacing: 0.03em;
		border-bottom: 1px solid rgba(201, 170, 113, 0.2);
		transition: border-color 0.2s;
	}

	.credit a:hover {
		border-color: #c9aa71;
	}

	.credit-desc {
		font-size: 0.95rem;
		color: #8a8a9a;
		font-weight: 300;
	}

	.credits-note {
		font-size: 0.9rem;
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
		max-width: 640px;
		margin: 0 auto;
		text-align: center;
	}

	.footer-text {
		font-size: 0.95rem;
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
		font-size: 0.95rem;
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

	/* === Transparency === */
	.transparency {
		position: relative;
		z-index: 2;
		max-width: 760px;
		margin: 0 auto;
		padding: 60px 24px 80px;
		border-top: 1px solid rgba(201, 170, 113, 0.08);
	}

	.transparency-intro {
		font-size: 1.15rem;
		line-height: 1.7;
		color: #b0b0be;
		font-weight: 300;
		text-align: center;
		margin-bottom: 48px;
	}

	.transparency-block {
		margin-bottom: 36px;
	}

	.transparency-subheading {
		font-family: 'Cinzel', serif;
		font-size: 1.15rem;
		font-weight: 700;
		color: #d4b87a;
		letter-spacing: 0.04em;
		margin-bottom: 16px;
	}

	.transparency-block p {
		font-size: 1.05rem;
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
		font-size: 0.95rem;
		font-weight: 700;
		color: #e0e0e0;
		letter-spacing: 0.02em;
		display: block;
		margin-bottom: 6px;
	}

	.transparency-item p {
		font-size: 1rem;
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
		font-size: 1.05rem;
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
		font-size: 0.95rem;
	}

	.transparency-table th {
		font-family: 'Cinzel', serif;
		font-size: 0.85rem;
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
		font-size: 1.05rem;
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
