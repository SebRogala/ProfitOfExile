import adapter from '@sveltejs/adapter-static';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		adapter: adapter({
			// The SPA shell for client-only routes. NOT index.html: the landing
			// page prerenders to index.html (routes/+page.ts) and a fallback of the
			// same name overwrote it with the empty shell. The Go static handler
			// serves this file for any path that is not a real file.
			fallback: '200.html'
		})
	}
};

export default config;
