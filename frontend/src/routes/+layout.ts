// adapter-static with fallback:'index.html' produces a pure client-side SPA.
// prerender generates the static shell at build time; ssr:false disables
// runtime server rendering (the Go backend serves pre-built files only).
// If future pages need SEO-friendly dynamic prerendering, revisit ssr:false.
export const prerender = true;
export const ssr = false;
