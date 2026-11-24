import { describe, it, expect } from 'vitest';
import pageSource from '$lib/pages/CurrencyExchangePage.svelte?raw';
import { EXCHANGE_TOOLTIPS } from './tooltips';

// The column headers the exchange table's <thead> hovers, in page order.
const HEADER_KEYS = [
	'Mode',
	'Route',
	'Investment',
	'ROI',
	'Exp. ROI',
	'ROI%',
	'Trend',
	'Depth',
	'Scale',
	'Hours'
];

const PLAIN_MAX_WORDS = 25;
const PLAIN_MAX_SENTENCES = 2;
const SENTENCE_MAX_WORDS = 20;
const GLOSS_MAX_WORDS = 8;
const ALLOWED_TAG = /^(<b>|<\/b>|<br>|<\/span>|<span style='color:[^'<>]+'>)$/;

function words(text: string): string[] {
	return text.split(/\s+/).filter((token) => /[\p{L}\p{N}]/u.test(token));
}

function sentences(text: string): string[] {
	return text
		.split(/(?<=[.!?])\s+/)
		.map((s) => s.trim())
		.filter((s) => words(s).length > 0);
}

function stripTags(html: string): string {
	return html.replace(/<[^>]*>/g, ' ');
}

function longSentences(key: string, text: string): string[] {
	return sentences(text)
		.map((s) => words(s).length)
		.filter((n) => n > SENTENCE_MAX_WORDS)
		.map((n) => `${key}: a sentence has ${n} words (max ${SENTENCE_MAX_WORDS})`);
}

function plainViolations(key: string, text: string): string[] {
	const out: string[] = [];
	const wordCount = words(text).length;
	const sentenceCount = sentences(text).length;
	if (wordCount > PLAIN_MAX_WORDS) {
		out.push(`${key}: plain text has ${wordCount} words (max ${PLAIN_MAX_WORDS})`);
	}
	if (sentenceCount > PLAIN_MAX_SENTENCES) {
		out.push(`${key}: plain text has ${sentenceCount} sentences (max ${PLAIN_MAX_SENTENCES})`);
	}
	return out.concat(longSentences(key, text));
}

function structuredViolations(key: string, text: string): string[] {
	const out: string[] = [];
	for (const tag of text.match(/<[^>]*>/g) ?? []) {
		if (!ALLOWED_TAG.test(tag)) out.push(`${key}: disallowed markup ${tag}`);
	}
	const [title, ...groups] = text.split('<br><br>');
	if (!/^<b>[^<]+<\/b>$/.test(title)) {
		out.push(`${key}: structured text must open with a bold title alone, got ${JSON.stringify(title)}`);
	}
	if (groups.length === 0) out.push(`${key}: structured text has 0 groups after the title`);
	groups.forEach((group, g) => {
		if (!group.startsWith('<b>')) out.push(`${key}: group ${g + 1} does not open with a bold heading`);
		const lines = group.split('<br>');
		const states = lines.filter((line) => line.trimStart().startsWith('•'));
		if (states.length === 0) out.push(`${key}: group ${g + 1} has 0 • state lines`);
		for (const line of lines) {
			out.push(...longSentences(key, stripTags(line)));
		}
		for (const line of states) {
			const plain = stripTags(line);
			const dash = plain.indexOf(' — ');
			if (dash < 0) {
				out.push(`${key}: state line has no " — " gloss: ${JSON.stringify(plain.trim())}`);
				continue;
			}
			const glossWords = words(plain.slice(dash + 3)).length;
			if (glossWords > GLOSS_MAX_WORDS) {
				out.push(`${key}: state gloss has ${glossWords} words (max ${GLOSS_MAX_WORDS})`);
			}
		}
	});
	return out;
}

function digestViolations(key: string, text: string): string[] {
	if (!text.includes('<')) return plainViolations(key, text);
	if (text.startsWith('<b>')) return structuredViolations(key, text);
	return [`${key}: text has markup but does not open with <b> (neither plain nor structured)`];
}

describe('EXCHANGE_TOOLTIPS column headers', () => {
	it.each(HEADER_KEYS)('keep:happy %s header tooltip is plain or structured within its bounds', (key) => {
		const text = EXCHANGE_TOOLTIPS[key];
		expect(text, `${key}: no EXCHANGE_TOOLTIPS entry`).toBeTypeOf('string');
		expect(digestViolations(key, text)).toEqual([]);
	});

	it('keep:happy the bounded key list is exactly the set the page thead renders', () => {
		const start = pageSource.indexOf('<thead>');
		const end = pageSource.indexOf('</thead>', start);
		expect(start, 'CurrencyExchangePage.svelte has no <thead>').toBeGreaterThanOrEqual(0);
		expect(end, 'CurrencyExchangePage.svelte has no </thead>').toBeGreaterThan(start);
		const thead = pageSource.slice(start, end);
		const access = /EXCHANGE_TOOLTIPS(?:\.([A-Za-z_$][\w$]*)|\[\s*'([^']+)'\s*\]|\[\s*"([^"]+)"\s*\])/g;
		const rendered = new Set([...thead.matchAll(access)].map((m) => m[1] ?? m[2] ?? m[3]));
		expect([...rendered].sort()).toEqual([...HEADER_KEYS].sort());
	});
});
