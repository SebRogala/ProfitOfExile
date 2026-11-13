/**
 * Vitest setup: every instance of the notice gate starts accepted, so a test
 * that delivers a server url sees it. A factory rather than a static import:
 * it runs again for each fresh module instance (`vi.resetModules`), and it
 * loads the real gate only after the test file's own `@tauri-apps` mocks are
 * registered. Tests of the withheld path (`gate.test.ts`,
 * `server-base.svelte.test.ts`) call `resetNoticeGateForTests()` first.
 */
import { vi } from 'vitest';

vi.mock('$lib/privacy/gate.svelte', async (importOriginal) => {
	const gate = await importOriginal<typeof import('./gate.svelte')>();
	gate.acceptNoticeForTests();
	return gate;
});
