import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	test: {
		environment: 'jsdom'
	},
	// @ts-expect-error process is available at Vite build time (Node)
	resolve: process.env.VITEST ? { conditions: ['browser'] } : undefined,
	server: {
		proxy: {
			'/api': {
				target: 'http://app:8080',
				changeOrigin: true
			}
		}
	}
});
