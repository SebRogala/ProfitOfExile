import { modules, PITCH } from '$lib/site-content';

/**
 * `/llms.txt` — the site in plain text, for LLM crawlers.
 *
 * Generated from the same `site-content` the page renders, so it cannot drift:
 * a new module reaches this file with its entry, and nothing has to be
 * remembered after a change. Nothing links to it, so `prerender` here is what
 * makes the build emit it as a static file for the Go handler to serve.
 */
export const prerender = true;

const SITE = 'https://profitofexile.top';
const REPO = 'https://github.com/SebRogala/ProfitOfExile';

function moduleSection(m: (typeof modules)[number]): string {
	const lines = [
		`### ${m.name}${m.status === 'beta' ? ' (beta)' : ''}`,
		'',
		m.tagline,
		'',
		`- Trigger: ${m.trigger}`,
		`- Reads: ${m.reads}`,
		...m.gives.map((g) => `- Gives: ${g}`),
	];
	if (m.setup) lines.push(`- Setup: ${m.setup}`);
	lines.push(`- On the site: ${SITE}/#module-${m.id}`);
	return lines.join('\n');
}

export function GET(): Response {
	const body = [
		'# ProfitOfExile',
		'',
		`> ${PITCH}`,
		'',
		'Windows desktop app for Path of Exile 1. It reads Client.txt to know when a',
		'panel is worth looking at, reads that panel with the OCR built into Windows,',
		'checks the market, and draws the verdict in a click-through overlay. It never',
		'reads game memory and never sends input to the game. Free and open source.',
		'',
		'## Modules',
		'',
		modules.map(moduleSection).join('\n\n'),
		'',
		'## Links',
		'',
		`- [Site](${SITE}/)`,
		`- [Installer](${REPO}/releases/latest/download/ProfitOfExile-setup.exe)`,
		`- [Portable build](${REPO}/releases/latest/download/ProfitOfExile-standalone.exe)`,
		`- [Source](${REPO})`,
		'- [Discord](https://discord.gg/QX53hrv5GP)',
		`- [What it does and does not do](${SITE}/#transparency)`,
		'',
	].join('\n');

	return new Response(body, {
		headers: { 'content-type': 'text/plain; charset=utf-8' },
	});
}
