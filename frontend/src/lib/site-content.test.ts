import { describe, it, expect } from 'vitest';
import { modules } from './site-content';
import { GET } from '../routes/llms.txt/+server';

describe('Mercenaries site copy', () => {
	it('does not call the merc trade lookup opt-in', () => {
		const merc = modules.find((m) => m.id === 'mercenaries');
		expect(merc).toBeDefined();
		expect(merc!.gives.join(' ')).not.toMatch(/opt-in/i);
	});

	it('keeps opt-in trade wording out of /llms.txt', async () => {
		const body = await GET().text();
		expect(body).toContain('### Mercenaries');
		expect(body).not.toMatch(/opt-in/i);
	});
});
