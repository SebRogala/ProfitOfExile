/**
 * The landing page's content: the one-sentence pitch and the module catalogue.
 *
 * It lives here rather than in `+page.svelte` because three surfaces derive
 * from it — the page, the JSON-LD in its head, and the generated `/llms.txt`.
 * Adding a module is adding an entry here plus its screenshots in `static/`.
 */

/** The one-sentence pitch. Shared by the meta description, Open Graph and the structured data. */
export const PITCH =
	'Companion app for Path of Exile 1: it reads your game log and screen, prices what is on offer, and shows the verdict in click-through in-game overlays.';

export interface Screenshot {
	/** Path under `static/`. */
	src: string;
	alt: string;
	caption: string;
}

export interface Module {
	/** Anchor: the entry renders as `#module-<id>`. */
	id: string;
	name: string;
	/** 'beta' modules are hidden until the device is promoted (POE-203); the section's footnote says how. */
	status: 'available' | 'beta';
	tagline: string;
	/** What arms it — a Client.txt event, a button, or nothing. */
	trigger: string;
	/** The screen region it reads, or "nothing". This line is what the Transparency section points at. */
	reads: string;
	/** What the player gets, one line each. */
	gives: string[];
	/** Anything the player must set up by hand for this module alone. */
	setup?: string;
	screenshots: Screenshot[];
}

/**
 * The Modules section, one entry per module. Adding a module is adding an
 * entry here plus its screenshots in `static/`; the rest of the page is
 * module-agnostic and should not need touching.
 */
export const modules: Module[] = [
	{
		id: 'lab',
		name: 'Lab Farming',
		status: 'available',
		tagline: 'Divine Font farming in the Labyrinth: which gem to take, and at what price.',
		trigger: "Entering the 3rd Aspirant's Trial, from Client.txt.",
		reads: 'The gem tooltip while you hover Font options, and the Font craft panel.',
		gives: [
			'Comparator overlay: the gems on offer with live trade prices, seller concentration and outliers, plus pick buttons.',
			'Path strip and compass overlays: your route through the lab with room contents and navigation cues.',
			'Font craft tracking: remaining uses and jackpots, with each session sent to the server to feed the dashboard.',
		],
		screenshots: [
			{
				src: '/overlay-comparator.webp',
				alt: 'Comparator overlay below the Divine Font panel: the three gems on offer, each with its trade price, a verdict badge, the weekly range and a pick button',
				caption: 'Comparator overlay — the three Font gems, priced and ranked',
			},
			{
				src: '/overlay-lab-path-compass.webp',
				alt: 'Path strip across the bottom of the lab showing the route and each room’s contents, with the compass at the right pointing to the northeast exit',
				caption: 'Path strip and compass — the route, what each room holds, and the exit to take',
			},
		],
	},
	{
		id: 'temple',
		name: 'Temple of Atzoatl',
		status: 'beta',
		tagline: "Alva's incursions: which architect to kill and which door to open.",
		trigger: "Alva's start line (\"Time to go\"), from Client.txt; any other Alva line or a zone change stands it down. The read needs the sheet on screen, so you open it yourself: inside the incursion, press the league button (V by default) — the game pauses while it is open, so it costs you nothing.",
		reads: 'The temple sheet — the 13 rooms and both architect offers — once per board.',
		gives: [
			'The ranked recommendation with its reasons, and the gambles with their measured risk.',
			'Room values in chaos, fed by the market (Default) or by your own numbers (Custom).',
		],
		// TODO screenshot: static/module-temple-page.png (the Temple page).
		screenshots: [
			{
				src: '/module-temple-overlay.webp',
				alt: 'Two offer boxes over the temple sheet, each with the room, its chaos value, the upgrade it pays for and the temple mod it grants',
				caption: 'Offer boxes over the temple sheet — both architects, priced',
			},
		],
	},
	{
		id: 'mercenaries',
		name: 'Mercenaries',
		status: 'beta',
		tagline: 'Is this recruit worth the wager?',
		trigger: "The recruit's voice line, from Client.txt — or Scan now on the page.",
		reads: 'The recruit window, row by row.',
		gives: [
			'A verdict per community guide ruleset, with per-row glyphs on an overlay strip that clears four seconds after the window closes.',
			'What the same mercenary is going for on trade (opt-in, with a searches-spent counter), plus links to the trade searches and a warrant price check.',
			'Gem icons it learns from the recruit window are pooled through the server as 24×24 signatures of the icon itself, so every device recognises them.',
		],
		// TODO screenshot: static/module-merc-page.png (the Mercenaries page).
		screenshots: [
			{
				src: '/module-merc-overlay.webp',
				alt: 'Verdict strip beside the recruit window: the mercenary’s name and level, a SKIP verdict, one row per skill with a glyph per gem, and a status line saying how many rows were read',
				caption: 'Verdict strip beside the recruit window — the call, then a glyph per skill row',
			},
		],
	},
	{
		id: 'exchange',
		name: 'Currency Exchange',
		status: 'beta',
		tagline: 'Arbitrage flips on the in-game Currency Exchange, ranked.',
		trigger: "None — the server ranks plays from GGG's public currency-exchange feed.",
		reads: 'Nothing on your screen.',
		gives: [
			'Each play as a five-step route — spend, buy, sell, convert, get — worded as orders the exchange will actually take.',
			'The worthwhile size derived for you: a scanner, not a calculator. Investment and ROI, net beside raw.',
		],
		screenshots: [
			{
				src: '/module-exchange-page.webp',
				alt: 'Ranked plays table: per row the mode, what you spend, the buy, sell and convert steps, what you get back, and investment beside ROI, expected ROI and ROI percent',
				caption: 'Currency Exchange page — plays ranked, each one spend → buy → sell → convert → get',
			},
		],
	},
];
