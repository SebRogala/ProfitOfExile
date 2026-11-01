import { describe, it, expect } from 'vitest';
import { baseGemTradeUrl, cheapestCorruptedTradeUrl, buyBaseUrl, baseGemNoQualityUrl } from './trade-utils';

const name = 'Kinetic Blast of Clustering';
const builders = [
	['base', (league?: string | null) => baseGemTradeUrl(name, '20/20', league)],
	['Dedication', (league?: string | null) => cheapestCorruptedTradeUrl('RED', false, league)],
	['Normal', (league?: string | null) => buyBaseUrl('20/20', 'RED', league)],
	['GCP', (league?: string | null) => baseGemNoQualityUrl(name, league)]
] as const;

function query(url: string | null) {
	expect(url).not.toBeNull();
	const parsed = new URL(url!);
	expect(parsed.origin).toBe('https://www.pathofexile.com');
	expect(parsed.pathname).toBe('/trade/search/New%20League');
	const q = JSON.parse(parsed.searchParams.get('q')!);
	expect(q.query.status).toEqual({ option: 'securable' });
	expect(q.sort).toEqual({ price: 'asc' });
	expect(q.query.filters.trade_filters.filters).toEqual({ sale_type: { option: 'priced' }, collapse: { option: 'true' } });
	return q.query;
}

for (const [label, build] of builders) {
	describe(label, () => {
		it.each([undefined, null, '', ' \t\n '])('disables the URL without a resolved league (%j)', league => {
			expect(build(league)).toBeNull();
		});
		it('encodes the resolved league name', () => { query(build('New League')); });
	});
}

describe('query preservation', () => {
	it.each(['1/0', '1/20', '20/0', '20/20'])('retains named base filters for %s', variant => {
		const q = query(baseGemTradeUrl(name, variant, 'New League'));
		expect(q.type).toBe('Kinetic Blast');
		expect(q.filters.type_filters.filters.category).toEqual({ option: 'gem' });
		expect(q.filters.misc_filters.filters).toEqual({
			corrupted: { option: 'false' },
			...(variant.startsWith('20/') ? { gem_level: { min: 20, max: 20 } } : {}),
			...(variant.endsWith('/20') ? { quality: { min: 20, max: 20 } } : {})
		});
	});
	it.each([
		['RED', { dex: { max: 97 }, int: { max: 97 } }],
		['GREEN', { str: { max: 97 }, int: { max: 97 } }],
		['BLUE', { str: { max: 97 }, dex: { max: 97 } }]
	] as const)('retains %s color caps in Normal and Dedication searches', (color, caps) => {
		for (const url of [buyBaseUrl('20/20', color, 'New League'), cheapestCorruptedTradeUrl(color, true, 'New League')]) {
			const q = query(url);
			expect(q.filters.req_filters.filters).toEqual(caps);
			expect(q.filters.type_filters.filters.category).toEqual({ option: 'gem.activegem' });
			expect(q.type).toBeUndefined();
		}
	});
	it.each(['20/0', '20/20'])('retains Normal minimum level and quality bounds for %s', variant => {
		const q = query(buyBaseUrl(variant, 'RED', 'New League'));
		expect(q.filters.misc_filters.filters).toEqual({ gem_level: { min: 20 }, corrupted: { option: 'false' }, quality: variant === '20/0' ? { max: 0 } : { min: 20 } });
	});
	it.each(['1/0', '1/20'])('disables color searches for level-one gems (%s)', variant => {
		expect(buyBaseUrl(variant, 'RED', 'New League')).toBeNull();
	});
	it.each(['21/20', '21/23'])('pins Dedication quality for both pools in %s', variant => {
		for (const transfigured of [false, true]) {
			const q = query(cheapestCorruptedTradeUrl('RED', transfigured, 'New League', variant));
			expect(q.filters.misc_filters.filters).toEqual({ gem_level: { min: 21 }, quality: { min: Number(variant.slice(3)), max: Number(variant.slice(3)) }, corrupted: { option: 'true' }, gem_transfigured: { option: String(transfigured) } });
		}
	});
	it('defaults Dedication to the 21/23 market', () => {
		expect(query(cheapestCorruptedTradeUrl('BLUE', false, 'New League')).filters.misc_filters.filters.quality).toEqual({ min: 23, max: 23 });
	});
	it('leaves GCP recipe quality unrestricted and level without a maximum', () => {
		const q = query(baseGemNoQualityUrl(name, 'New League'));
		expect(q.type).toBe('Kinetic Blast');
		expect(q.filters.type_filters.filters.category).toEqual({ option: 'gem' });
		expect(q.filters.misc_filters.filters).toEqual({ gem_level: { min: 20 }, corrupted: { option: 'false' } });
	});
});
