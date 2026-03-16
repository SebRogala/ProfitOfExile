/**
 * Lab Dashboard API service — mock data for v1.
 * Each function matches a real endpoint shape so swapping to fetch() is trivial.
 */

// --- Types ---

export interface StatusData {
	lastUpdate: string;
	nextFetch: string;
	connected: boolean;
	collectorUptime: string;
}

export type PriceTier = 'TOP' | 'MID' | 'LOW';
export type SellUrgency = 'SELL_NOW' | 'UNDERCUT' | 'HOLD' | 'WAIT' | '';
export type SellabilityLabel = 'FAST SELL' | 'GOOD' | 'MODERATE' | 'SLOW' | 'UNLIKELY';

export interface GemPlay {
	name: string;
	variant: string;
	color: 'RED' | 'GREEN' | 'BLUE';
	roi: number;
	roiPercent: number;
	signal: string;
	cv: number;
	windowSignal: string;
	advancedSignal: string;
	liquidityTier: string;
	transListings: number;
	transVelocity: number;
	baseListings: number;
	baseVelocity: number;
	basePrice: number;
	transPrice: number;
	sparkline: number[];
	signalHistory: SignalTransition[];
	priceTier: PriceTier;
	tierAction: string;
	sellUrgency: SellUrgency;
	sellReason: string;
	sellability: number;
	sellabilityLabel: SellabilityLabel;
}

export interface SignalTransition {
	time: string;
	from: string;
	to: string;
	reason: string;
	listings: number;
}

export interface FontColor {
	color: 'RED' | 'GREEN' | 'BLUE';
	ev: number;
	pool: number;
	winners: number;
	pWin: number;
	profit: number;
	evDelta2h: number;
}

export interface FontEVData {
	colors: FontColor[];
	qualityAvgRoi: number;
	bestColor: string;
	bestAdvantage: number;
}

export interface WindowAlert {
	windowSignal: string;
	name: string;
	variant: string;
	roi: number;
	transListings: number;
	baseListings: number;
	baseVelocity: number;
	liquidityTier: string;
	action: string;
	history: SignalTransition[];
}

export interface MarketOverviewData {
	avgTransPrice: number;
	avgTransPriceDelta: number;
	avgBaseListings: number;
	avgBaseListingsDelta: number;
	activeGems: number;
	weekendPremium: number;
	windowGems: number;
	windowBreakdown: Record<string, number>;
	trapGems: number;
	mostVolatileColor: string;
	mostVolatileCV: number;
	mostStableColor: string;
	mostStableCV: number;
}

export interface CompareGem {
	name: string;
	variant: string;
	color: 'RED' | 'GREEN' | 'BLUE';
	roi: number;
	roiPercent: number;
	signal: string;
	cv: number;
	transListings: number;
	transVelocity: number;
	baseListings: number;
	baseVelocity: number;
	liquidityTier: string;
	windowSignal: string;
	sparkline: number[];
	signalHistory: SignalTransition[];
	recommendation: 'BEST' | 'OK' | 'AVOID';
	priceTier: PriceTier;
	tierAction: string;
	sellUrgency: SellUrgency;
	sellReason: string;
	sellability: number;
	sellabilityLabel: SellabilityLabel;
}

// --- Mock Data ---

const MOCK_STATUS: StatusData = {
	lastUpdate: '2026-03-15T14:15:00Z',
	nextFetch: '2026-03-15T14:45:00Z',
	connected: true,
	collectorUptime: '18h 32m',
};

const MOCK_GEM_NAMES = [
	'Spark of the Nova',
	'Spark of Unpredictability',
	'Lightning Strike of Arcing',
	'Ball Lightning of Orbiting',
	'Ball Lightning of Static',
	'Arc of Surging',
	'Arc of Oscillating',
	'Ethereal Knives of the Massacre',
	'Blade Vortex of the Scythe',
	'Ice Nova of Frostbolts',
	'Vaal Grace',
	'Molten Strike of the Zenith',
	'Cyclone of Tumult',
	'Tornado Shot of Cloudburst',
	'Ice Shot of Penetration',
	'Lightning Arrow of Electrocution',
	'Rain of Arrows of Artillery',
	'Caustic Arrow of Poison',
	'Spectral Helix of Trarthus',
	'Boneshatter of Complex Trauma',
];

function makeSparkline(): number[] {
	const base = 40 + Math.random() * 60;
	return Array.from({ length: 8 }, () => Math.max(5, base + (Math.random() - 0.5) * 40));
}

function makeHistory(): SignalTransition[] {
	const base = 30 + Math.floor(Math.random() * 80);
	return [
		{ time: '13:15', from: 'STABLE', to: 'RISING', reason: '+8c/h', listings: base + 12 },
		{ time: '13:45', from: 'RISING', to: 'RISING', reason: '+6c/h', listings: base + 4 },
		{ time: '14:15', from: 'RISING', to: 'STABLE', reason: 'velocity settled', listings: base },
	];
}

const SIGNALS = ['STABLE', 'RISING', 'FALLING', 'HERD', 'DUMPING', 'RECOVERY', 'TRAP'];
const WINDOWS = ['CLOSED', 'BREWING', 'OPENING', 'OPEN', 'CLOSING', 'EXHAUSTED'];
const ADVANCED = ['', '', '', 'COMEBACK', 'POTENTIAL', 'PRICE_MANIPULATION', 'BREAKOUT'];
const COLORS: Array<'RED' | 'GREEN' | 'BLUE'> = ['RED', 'GREEN', 'BLUE'];
const VARIANTS = ['1/0', '1/20', '20/0', '20/20'];
const LIQUIDITY = ['HIGH', 'MED', 'LOW'];
const TIERS: PriceTier[] = ['TOP', 'MID', 'LOW'];
const SELL_URGENCIES: SellUrgency[] = ['', '', '', 'HOLD', 'WAIT', 'UNDERCUT', 'SELL_NOW'];

const TIER_ACTIONS: Record<PriceTier, string[]> = {
	TOP: ['SELL — move is over', 'WATCH — early stage, monitor closely', 'RIDE — strong momentum'],
	MID: ['CAUTIOUS — may reverse', 'SELL — exit position', 'HOLD — building momentum'],
	LOW: ['UNRELIABLE — low-value windows are traps', 'SKIP — not worth the risk', 'WAIT — needs confirmation'],
};

const SELL_REASONS: Record<string, string> = {
	SELL_NOW: 'Price crashing — sell at any price before further drop',
	UNDERCUT: 'Market cooling — undercut 10-15% for fast sale',
	HOLD: 'Price rising steadily — no rush to sell',
	WAIT: 'No clear signal — wait for better conditions',
};

function makeSellability(): { sellability: number; sellabilityLabel: SellabilityLabel } {
	const s = Math.floor(Math.random() * 100);
	let label: SellabilityLabel;
	if (s >= 80) label = 'FAST SELL';
	else if (s >= 60) label = 'GOOD';
	else if (s >= 40) label = 'MODERATE';
	else if (s >= 20) label = 'SLOW';
	else label = 'UNLIKELY';
	return { sellability: s, sellabilityLabel: label };
}

function pickRandom<T>(arr: T[]): T {
	return arr[Math.floor(Math.random() * arr.length)];
}

function generatePlay(name: string, variant?: string): GemPlay {
	const basePrice = 5 + Math.floor(Math.random() * 80);
	const transPrice = basePrice + 20 + Math.floor(Math.random() * 600);
	const roi = transPrice - basePrice;
	const priceTier = pickRandom(TIERS);
	const sellUrgency = pickRandom(SELL_URGENCIES);
	const { sellability, sellabilityLabel } = makeSellability();
	return {
		name,
		variant: variant || pickRandom(VARIANTS),
		color: pickRandom(COLORS),
		roi,
		roiPercent: Math.round((roi / basePrice) * 100),
		signal: pickRandom(SIGNALS.slice(0, 3)),
		cv: 5 + Math.floor(Math.random() * 60),
		windowSignal: pickRandom(WINDOWS.slice(0, 3)),
		advancedSignal: pickRandom(ADVANCED),
		liquidityTier: pickRandom(LIQUIDITY),
		transListings: 10 + Math.floor(Math.random() * 200),
		transVelocity: Math.round((Math.random() - 0.5) * 20),
		baseListings: 20 + Math.floor(Math.random() * 300),
		baseVelocity: Math.round((Math.random() - 0.5) * 30),
		basePrice,
		transPrice,
		sparkline: makeSparkline(),
		signalHistory: makeHistory(),
		priceTier,
		tierAction: pickRandom(TIER_ACTIONS[priceTier]),
		sellUrgency,
		sellReason: sellUrgency ? SELL_REASONS[sellUrgency] || '' : '',
		sellability,
		sellabilityLabel,
	};
}

function generatePlays(count: number, variant?: string): GemPlay[] {
	const plays: GemPlay[] = [];
	const used = new Set<string>();
	for (let i = 0; i < count; i++) {
		let name = pickRandom(MOCK_GEM_NAMES);
		while (used.has(name)) name = pickRandom(MOCK_GEM_NAMES);
		used.add(name);
		plays.push(generatePlay(name, variant));
	}
	return plays.sort((a, b) => b.roi - a.roi);
}

// Stable mock data (generated once)
let _bestPlays: GemPlay[] | null = null;
let _variantPlays: Record<string, GemPlay[]> = {};
let _fontEV: Record<string, FontEVData> = {};
let _windowAlerts: WindowAlert[] | null = null;
let _marketOverview: MarketOverviewData | null = null;

function getBestPlays(): GemPlay[] {
	if (!_bestPlays) {
		_bestPlays = generatePlays(15);
		// Ensure variety of signals for demo
		_bestPlays[0].signal = 'RISING';
		_bestPlays[0].windowSignal = 'OPEN';
		_bestPlays[0].advancedSignal = '';
		_bestPlays[0].sellUrgency = 'HOLD';
		_bestPlays[0].sellReason = SELL_REASONS.HOLD;
		_bestPlays[0].priceTier = 'TOP';
		_bestPlays[0].tierAction = 'RIDE — strong momentum';
		_bestPlays[0].sellability = 92;
		_bestPlays[0].sellabilityLabel = 'FAST SELL';
		_bestPlays[1].signal = 'STABLE';
		_bestPlays[1].windowSignal = 'BREWING';
		_bestPlays[1].advancedSignal = 'POTENTIAL';
		_bestPlays[1].priceTier = 'MID';
		_bestPlays[1].tierAction = 'HOLD — building momentum';
		_bestPlays[2].signal = 'RISING';
		_bestPlays[2].advancedSignal = 'COMEBACK';
		_bestPlays[2].sellUrgency = 'UNDERCUT';
		_bestPlays[2].sellReason = SELL_REASONS.UNDERCUT;
		_bestPlays[3].signal = 'FALLING';
		_bestPlays[3].windowSignal = 'CLOSING';
		_bestPlays[3].sellUrgency = 'SELL_NOW';
		_bestPlays[3].sellReason = SELL_REASONS.SELL_NOW;
		_bestPlays[3].sellability = 12;
		_bestPlays[3].sellabilityLabel = 'UNLIKELY';
		_bestPlays[4].signal = 'HERD';
		_bestPlays[5].signal = 'DUMPING';
		_bestPlays[5].sellUrgency = 'SELL_NOW';
		_bestPlays[5].sellReason = SELL_REASONS.SELL_NOW;
		_bestPlays[6].signal = 'RECOVERY';
		_bestPlays[6].advancedSignal = 'BREAKOUT';
	}
	return _bestPlays;
}

function getVariantPlays(variant: string): GemPlay[] {
	if (!_variantPlays[variant]) {
		_variantPlays[variant] = generatePlays(8, variant);
	}
	return _variantPlays[variant];
}

function getFontEV(variant: string): FontEVData {
	if (!_fontEV[variant]) {
		_fontEV[variant] = {
			colors: [
				{ color: 'RED', ev: 280 + Math.floor(Math.random() * 200), pool: 28 + Math.floor(Math.random() * 10), winners: 3 + Math.floor(Math.random() * 5), pWin: 15 + Math.floor(Math.random() * 25), profit: 200 + Math.floor(Math.random() * 300), evDelta2h: Math.round((Math.random() - 0.5) * 30) },
				{ color: 'GREEN', ev: 180 + Math.floor(Math.random() * 150), pool: 38 + Math.floor(Math.random() * 15), winners: 4 + Math.floor(Math.random() * 6), pWin: 12 + Math.floor(Math.random() * 20), profit: 100 + Math.floor(Math.random() * 250), evDelta2h: Math.round((Math.random() - 0.5) * 20) },
				{ color: 'BLUE', ev: 150 + Math.floor(Math.random() * 120), pool: 45 + Math.floor(Math.random() * 15), winners: 5 + Math.floor(Math.random() * 7), pWin: 10 + Math.floor(Math.random() * 18), profit: 80 + Math.floor(Math.random() * 200), evDelta2h: Math.round((Math.random() - 0.5) * 15) },
			],
			qualityAvgRoi: 40 + Math.floor(Math.random() * 80),
			bestColor: 'RED',
			bestAdvantage: 80 + Math.floor(Math.random() * 200),
		};
	}
	return _fontEV[variant];
}

function getWindowAlerts(): WindowAlert[] {
	if (!_windowAlerts) {
		_windowAlerts = [
			{
				windowSignal: 'OPEN',
				name: 'Spark of the Nova',
				variant: '20/20',
				roi: 940,
				transListings: 12,
				baseListings: 45,
				baseVelocity: -8,
				liquidityTier: 'MED',
				action: 'Farm NOW! Bases draining fast, transfigured supply is low.',
				history: [
					{ time: '13:15', from: 'BREWING', to: 'OPENING', reason: 'base velocity -4/h', listings: 18 },
					{ time: '13:45', from: 'OPENING', to: 'OPEN', reason: 'window score 78', listings: 12 },
				],
			},
			{
				windowSignal: 'BREWING',
				name: 'Ball Lightning of Static',
				variant: '20/0',
				roi: 620,
				transListings: 28,
				baseListings: 89,
				baseVelocity: -3,
				liquidityTier: 'HIGH',
				action: 'Opportunity forming. Start planning your lab run.',
				history: [
					{ time: '14:00', from: 'CLOSED', to: 'BREWING', reason: 'price +6c/h, listings -2/h', listings: 28 },
				],
			},
		];
	}
	return _windowAlerts;
}

function getMarketOverview(): MarketOverviewData {
	if (!_marketOverview) {
		_marketOverview = {
			avgTransPrice: 82,
			avgTransPriceDelta: 3,
			avgBaseListings: 127,
			avgBaseListingsDelta: -8,
			activeGems: 170,
			weekendPremium: 30,
			windowGems: 2,
			windowBreakdown: { OPEN: 1, BREWING: 1 },
			trapGems: 8,
			mostVolatileColor: 'BLUE',
			mostVolatileCV: 45,
			mostStableColor: 'RED',
			mostStableCV: 28,
		};
	}
	return _marketOverview;
}

// --- Public API functions ---

export async function fetchStatus(): Promise<StatusData> {
	return MOCK_STATUS;
}

export async function fetchBestPlays(
	_variant?: string,
	_budget?: number,
	_sort?: 'roi' | 'roiPercent'
): Promise<GemPlay[]> {
	const plays = getBestPlays().filter((p) => p.signal !== 'TRAP');
	return plays;
}

export async function fetchVariantPlays(variant: string): Promise<GemPlay[]> {
	return getVariantPlays(variant);
}

export async function fetchFontEV(variant: string): Promise<FontEVData> {
	return getFontEV(variant);
}

export async function fetchWindowAlerts(): Promise<WindowAlert[]> {
	return getWindowAlerts();
}

export async function fetchMarketOverview(): Promise<MarketOverviewData> {
	return getMarketOverview();
}

export async function fetchGemNames(query: string): Promise<string[]> {
	const q = query.toLowerCase();
	return MOCK_GEM_NAMES.filter((n) => n.toLowerCase().includes(q));
}

export async function fetchCompare(gems: string[], variant: string): Promise<CompareGem[]> {
	const recommendations: Array<'BEST' | 'OK' | 'AVOID'> = ['BEST', 'OK', 'AVOID'];
	const urgencies: SellUrgency[] = ['HOLD', '', 'SELL_NOW'];
	return gems.map((name, i) => {
		const priceTier = TIERS[i % 3];
		const sellUrgency = urgencies[i] || '';
		const { sellability, sellabilityLabel } = makeSellability();
		return {
			name,
			variant,
			color: COLORS[i % 3],
			roi: 200 + Math.floor(Math.random() * 800),
			roiPercent: 100 + Math.floor(Math.random() * 900),
			signal: pickRandom(['STABLE', 'RISING']),
			cv: 10 + Math.floor(Math.random() * 40),
			transListings: 15 + Math.floor(Math.random() * 100),
			transVelocity: Math.round((Math.random() - 0.5) * 10),
			baseListings: 30 + Math.floor(Math.random() * 200),
			baseVelocity: Math.round((Math.random() - 0.5) * 15),
			liquidityTier: pickRandom(LIQUIDITY),
			windowSignal: pickRandom(['CLOSED', 'BREWING']),
			sparkline: makeSparkline(),
			signalHistory: makeHistory(),
			recommendation: recommendations[i] || 'OK',
			priceTier,
			tierAction: pickRandom(TIER_ACTIONS[priceTier]),
			sellUrgency,
			sellReason: sellUrgency ? SELL_REASONS[sellUrgency] || '' : '',
			sellability,
			sellabilityLabel,
		};
	});
}
