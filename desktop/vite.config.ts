import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// @ts-expect-error process is available at Vite build time (Node)
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
	plugins: [sveltekit()],
	clearScreen: false,
	server: {
		port: 1420,
		strictPort: true,
		host: host || false,
		hmr: host
			? {
					protocol: 'ws',
					host,
					port: 1421
				}
			: undefined,
		watch: {
			ignored: ['**/src-tauri/**'],
			// The dev tree is written by `make desktop-watch`, which rsyncs from WSL
			// into a Windows directory. Observed 2026-08-08: an edit landed in the
			// Windows tree but Vite never rebuilt, so HMR kept serving a module
			// compiled from the previous version — the running app threw
			// `searchQuery is not defined` from code that no longer existed on disk,
			// which reads as an application bug rather than a stale build.
			//
			// Native filesystem events do not cross the WSL→/mnt/c boundary
			// reliably, and rsync's write-temp-then-rename makes it worse. Polling
			// is the only watch mode that sees those writes.
			//
			// Accepted trade-off: a poll every 400ms costs idle CPU that native
			// events would not. That is worth less than one debugging session spent
			// on a file that was already correct.
			usePolling: true,
			interval: 400
		}
	}
});
