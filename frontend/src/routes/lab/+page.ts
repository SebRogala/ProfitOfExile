// Prerendered to full HTML like the landing page (see routes/+page.ts): link
// previews and crawlers read the <svelte:head> tags without running JS. The
// page guards its window/localStorage reads with typeof checks and fetches in
// onMount/$effect, so the build server renders the empty-data state.
export const ssr = true;
