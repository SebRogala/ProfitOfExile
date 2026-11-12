// The node env compiles `.svelte.ts` for the server, where `$effect` is a no-op; this compiles it for the client without a DOM.
export default { name: 'node-client', viteEnvironment: 'client', setup: () => ({ teardown() {} }) };
