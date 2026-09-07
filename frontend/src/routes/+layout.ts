// adapter-static with fallback:'200.html' produces a client-side SPA shell.
// prerender generates the static files at build time; ssr:false disables
// runtime server rendering (the Go backend serves pre-built files only).
// The landing page overrides ssr in routes/+page.ts so it prerenders to full
// HTML; any other page that needs to be readable without JS does the same.
export const prerender = true;
export const ssr = false;
