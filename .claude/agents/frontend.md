# Frontend Agent

UI implementation principles. Extends the general agent with SvelteKit, component, and styling conventions.

## SvelteKit Conventions

- SvelteKit with `adapter-static` — no Node runtime in production. Built to static files served by Go backend.
- File-based routing in `src/routes/`. Page components in `+page.svelte`, layouts in `+layout.svelte`.
- Data loading via `+page.ts` (client-side) or `+page.server.ts` (server-side) load functions.
- Use Svelte stores for shared client state. Prefer derived stores over manual subscriptions.

## Component Structure

- Follow the project's existing component organization. Match naming conventions and directory structure.
- Keep components focused on presentation. Business logic and data transformation belong in stores or utility modules, not inline in templates.
- Extract repeated UI patterns into reusable components in `src/lib/components/`. But don't prematurely abstract — wait until a pattern appears at least twice.
- Props use `export let` declarations with TypeScript types. Events use `createEventDispatcher`.

## Styling (Tailwind CSS)

- Use Tailwind utility classes for styling. Follow the project's `tailwind.config.js` values.
- Spacing, colors, and typography follow the design system. Don't use arbitrary values when a design token exists.
- Common layout patterns:
  - Flex: `flex items-center gap-4`
  - Grid: `grid grid-cols-2 gap-6`
- Responsive design: test at standard breakpoints. Desktop-first but should degrade gracefully.

## Client-Side Interactions

- Use Svelte reactive declarations (`$:`) for derived state, not manual event listeners.
- Handle loading states explicitly. Show feedback when processing — disable buttons, show spinners.
- Clean up subscriptions and timers in `onDestroy` lifecycle hooks.

## Accessibility

- Interactive elements must be keyboard accessible.
- Form inputs need associated labels. Error messages linked to their fields.
- Use semantic HTML elements (nav, main, section, button) over generic divs with click handlers.
- Color should not be the only indicator of state. Combine with text, icons, or other cues.

## Form Handling

- Display validation errors near the relevant fields, not just as a generic banner.
- Preserve user input on validation failure. Don't clear the form when submission fails.
- Use SvelteKit form actions or fetch-based submission depending on the interaction pattern.
- Distinguish between field-level errors and form-level errors. Display each appropriately.

## Data Fetching

- API calls to the Go backend use `fetch` from load functions or reactive statements.
- Handle error states explicitly — show meaningful messages, not silent failures.
- Cache-aware: strategy and price data has TTLs. Show staleness indicators when data is cached.

## Domain-Specific UI

- **Strategy Tree Editor**: Composable tree visualization. Nodes represent farming activities with inputs, outputs, series counts. Some nodes have sub-strategies (e.g., Lab Run contains enchant-specific branches). Show gem type constraints (skill-only, support-only, both).
- **Price Display**: Always show source alongside price. Color-code buy (lowest) vs sell (highest) sources. Show listing count as a confidence indicator — <5 listings = low confidence warning. Support time-of-day context (peak vs dip hours).
- **Profitability Dashboard**: c/hr (chaos per hour) as the primary metric. Breakpoint charts showing optimal sell depths. Show risk level alongside profit (based on listing velocity, saturation signals).
- **Inventory View**: Real-time inventory state during simulation. Highlight auto-buy triggers and set conversions.
- **Market Risk Indicators**: Listing velocity trends (rising listings = saturation risk). Price crash/recovery patterns. Saturation watchlist for strategies to avoid. Visual signals: listing count doubling overnight is a red flag.
- **Variant Selector**: When choosing strategy inputs, show all available variants (20/20, 20/0, 1/20) with their ROI comparison. Highlight the best variant for each strategy.
- **Decision Flow Visualization**: For strategies with choices (Lab enchant selection), show the decision tree with ROI at each branch. Help users understand which enchant to pick for each gem they're carrying.
