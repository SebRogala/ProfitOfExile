---
name: frontend
description: Use for the browser-facing SvelteKit application under frontend/. Applies Svelte 5 runes, adapter-static constraints, Tailwind CSS v4 configuration, accessibility, API error handling, and the existing component and route structure.
---

# Web frontend agent

Work under `frontend/`; desktop UI has its own profile and conventions.

- Use Svelte 5 runes: `$props`, `$state`, `$derived`, and `$effect` as appropriate.
  Do not introduce Svelte 4 `export let`, `$:` conventions, or
  `createEventDispatcher` into rune-based components.
- Preserve `adapter-static`. Avoid request-time server-only assumptions; the
  production frontend is static and served by Go.
- Tailwind v4 is configured through Vite and CSS in `frontend/src/app.css`;
  there is no `tailwind.config.js`.
- Follow existing routes and components before adding abstractions. Extract a
  component when it is shared or materially clarifies a page.
- Keep API loading, empty, stale, and failure states explicit.
- Use semantic HTML, keyboard-accessible controls, associated form labels, and
  more than color alone to communicate state.

The strategy-tree editor, inventory simulation, breakpoint visualization, and
generic decision tree are historical product vision, not current frontend
surfaces. Confirm current routes and API contracts in code.

For a frontend source, config, or dependency change, run `npm run check` (CI's "Svelte check" step in
`.github/workflows/quality.yml`) as well as the production build `npm run build`: `vite build`
compiles without typechecking, so an undefined identifier builds green and fails at runtime in prod.
