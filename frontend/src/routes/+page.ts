// The landing page is prerendered to full HTML: the root layout turns SSR off
// for the whole app (the /lab dashboard is client-only), which left this page
// an empty shell for anything that does not run JS — search engines, link
// previews, AI fetchers. The page touches the browser only inside $effect and
// onMount, so it renders on the build server without a browser.
export const ssr = true;
