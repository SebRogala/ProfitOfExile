import { describe, expect, it } from 'vitest';

/**
 * The mechanical half of the AGENTS.md mirror rule (POE-345): every file that
 * has a same-named copy under the two mirrored roots is either byte-identical
 * by contract (`IDENTICAL`) or allowed to differ for a stated reason
 * (`DIVERGENT`). A change that makes a pair identical or divergent updates
 * these lists in the same change.
 *
 * Sources are read through Vite's `?raw` glob rather than `node:fs`: the
 * desktop tree has no `@types/node`, and the glob also gives the discovery.
 * The web copies are read from their canonical path; docker-compose.yml mounts
 * `./frontend/src` into the desktop container for that.
 */

// Literal glob strings: Vite resolves them at transform time. The `(app)`
// segment is escaped because picomatch reads `(…)` as a group.
const DESKTOP_LIB = './';
const WEB_LIB = '../../../frontend/src/lib/';
const DESKTOP_COMPONENTS = '../routes/(app)/components/';
const WEB_COMPONENTS = '../../../frontend/src/routes/lab/components/';

const desktopLib = import.meta.glob<string>('./**/*', { query: '?raw', import: 'default', eager: true });
const webLib = import.meta.glob<string>('../../../frontend/src/lib/**/*', { query: '?raw', import: 'default', eager: true });
const desktopComponents = import.meta.glob<string>('../routes/\\(app\\)/components/**/*', { query: '?raw', import: 'default', eager: true });
const webComponents = import.meta.glob<string>('../../../frontend/src/routes/lab/components/**/*', { query: '?raw', import: 'default', eager: true });

type Side = Map<string, string>;

function byRelativePath(files: Record<string, string>, root: string): Side {
	const side: Side = new Map();
	for (const [path, source] of Object.entries(files)) {
		if (!path.startsWith(root)) throw new Error(`glob key ${path} is outside root ${root}`);
		side.set(path.slice(root.length), source);
	}
	return side;
}

interface MirrorRoot {
	name: string;
	desktop: Side;
	web: Side;
	identical: string[];
	divergent: Record<string, string>;
}

const ROOTS: MirrorRoot[] = [
	{
		name: 'lib',
		desktop: byRelativePath(desktopLib, DESKTOP_LIB),
		web: byRelativePath(webLib, WEB_LIB),
		identical: ['trade-utils.ts', 'trade-utils.test.ts', 'price.svelte.ts', 'session-queue.ts', 'comparator/league-fence.ts', 'comparator/league-fence.test.ts'],
		divergent: {
			'api.ts':
				'shared open contract (MercureHandlers names, onOpen on every open after onConnectionChange(true)); bodies differ by topic set (desktop 5, web 2), web trade dispatch, and token base/headers (desktop Tauri, web same-origin)',
			'api.test.ts': 'desktop mocks Tauri invoke/proxy; web stubs fetch; the shared collectiveParams cases are identical',
			'tradeApi.ts': 'desktop native trade queue vs web HTTP lookup + listener/poll',
			'icons.ts': 'desktop configured server base vs web same-origin /api',
			'tooltips.ts': 'desktop also carries the Currency Exchange EXCHANGE_TOOLTIPS table; the web has no exchange page',
			'prefs.svelte.ts': 'desktop persists through the Rust ui_prefs map (load, buffered pre-load writes, debounce); web uses localStorage',
			'components/Select.svelte': 'desktop `id` prop + full width',
		},
	},
	{
		name: 'components',
		desktop: byRelativePath(desktopComponents, DESKTOP_COMPONENTS),
		web: byRelativePath(webComponents, WEB_COMPONENTS),
		identical: ['GemIcon.svelte', 'Legend.svelte', 'OfferingChart.svelte', 'Sparkline.svelte'],
		divergent: {
			'Comparator.svelte': 'desktop shell over the Tauri-bound controller; web owns its state inline',
			'FontEVCompare.svelte':
				'desktop reads and writes variant/dedication selection through the ssot store with SegmentedButtons and Tooltip; web takes a league prop and keeps local Select state',
			'ByVariant.svelte':
				'desktop tab follows the ssot normal variant and uses the shared Tooltip; web keeps a local tab, a league prop and InfoTooltip',
			'InfoTooltip.svelte': 'desktop wraps the shared Tooltip component; web implements its own hover/pin panel',
			'BestPlays.svelte':
				'desktop reads ssot and loads the low-confidence toggle from Tauri settings with the shared Tooltip; web takes a league prop and uses InfoTooltip',
			'Header.svelte': 'web header carries the Normal/Dedication lab selector and a title attribute; desktop uses the shared Tooltip and no lab selector',
			'FontEV.svelte': 'desktop shared Tooltip vs web InfoTooltip',
			'SignalBadge.svelte': 'desktop Tooltip vs web title attribute',
			'MarketOverview.svelte': 'desktop $effect vs web onDestroy ticker',
			'SessionQueue.svelte': 'trade refresh is desktop-only',
		},
	},
];

const WEB_TREE = 'frontend/src';

type MirrorCheckPlan = { run: true } | { run: false; reason: string };

/**
 * A desktop-only tree (e.g. the Windows rsync of `desktop/`) has no web copies
 * at all; that is an absent tree, not a stale list, so the guard skips there.
 * Any web file present means the tree exists and every rule runs.
 */
function mirrorCheckPlan(webTreeFiles: string[]): MirrorCheckPlan {
	if (webTreeFiles.length > 0) return { run: true };
	return { run: false, reason: `mirror identity check did not run: no web files found under ${WEB_TREE}` };
}

describe('mirrorCheckPlan', () => {
	it('skips with a reason naming the web tree when no web file is discovered', () => {
		expect(mirrorCheckPlan([])).toEqual({
			run: false,
			reason: 'mirror identity check did not run: no web files found under frontend/src',
		});
	});

	it('runs when the web tree has files only outside the mirrored roots', () => {
		expect(mirrorCheckPlan(['../../../frontend/src/routes/+page.svelte'])).toEqual({ run: true });
	});
});

// Keys only (lazy loaders): absence is decided on the whole web tree, not on the mirrored roots.
const webTree = Object.keys(import.meta.glob('../../../frontend/src/**/*'));
const plan = mirrorCheckPlan(webTree);
if (!plan.run) console.warn(plan.reason);

function pairsOf(root: MirrorRoot): string[] {
	return [...root.desktop.keys()].filter((key) => root.web.has(key)).sort();
}

describe.skipIf(!plan.run).each(ROOTS)('mirror identity: $name', (root) => {
	const listed = [...root.identical, ...Object.keys(root.divergent)];

	it.each(root.identical)('IDENTICAL %s is byte-identical across the two copies', (key) => {
		expect(root.desktop.get(key) === root.web.get(key), `${root.name}/${key} differs between desktop and web`).toBe(true);
	});

	it.each(Object.keys(root.divergent))('DIVERGENT %s still differs (else promote to IDENTICAL)', (key) => {
		expect(root.desktop.get(key) === root.web.get(key), `${root.name}/${key} is identical: promote to IDENTICAL`).toBe(false);
	});

	it.each(pairsOf(root))('discovered pair %s is listed', (key) => {
		expect(listed, `${root.name}/${key}: list it with a reason or make it identical`).toContain(key);
	});

	it.each(listed)('listed entry %s exists on both sides', (key) => {
		expect(root.desktop.has(key), `${root.name}/${key} missing on desktop: stale list`).toBe(true);
		expect(root.web.has(key), `${root.name}/${key} missing on web: stale list`).toBe(true);
	});

	it.each(root.identical)('IDENTICAL %s is not also DIVERGENT', (key) => {
		expect(Object.keys(root.divergent), `${root.name}/${key} is in both lists`).not.toContain(key);
	});

	it.each(Object.entries(root.divergent))('DIVERGENT %s has a reason', (key, reason) => {
		expect(reason.trim(), `${root.name}/${key} has an empty reason`).not.toBe('');
	});
});
