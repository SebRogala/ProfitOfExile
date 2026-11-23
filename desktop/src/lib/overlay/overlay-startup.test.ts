import { expect, it } from 'vitest';
import ts from 'typescript';

const sources = import.meta.glob('/src/**/*.{ts,svelte}', {
	query: '?raw', import: 'default', eager: true
}) as Record<string, string>;

type Module = {
	ast: ts.SourceFile;
	edges: string[];
	bindings: Map<string, { target: string; exported: string }>;
	lexical: Set<string>;
};

function resolve(from: string, specifier: string): string | null {
	if (!specifier.startsWith('.') && !specifier.startsWith('$lib/')) return null;
	const parts = (specifier.startsWith('$lib/')
		? `/src/lib/${specifier.slice(5)}`
		: `${from.slice(0, from.lastIndexOf('/'))}/${specifier}`).split('/');
	const normalized: string[] = [];
	for (const part of parts) {
		if (part === '..') normalized.pop();
		else if (part && part !== '.') normalized.push(part);
	}
	const base = `/${normalized.join('/')}`;
	const found = [base, `${base}.ts`, `${base}/index.ts`].find((path) => path in sources);
	if (!found && !/\.(css|json|svg|png)(\?|$)/.test(base)) {
		throw new Error(`Unresolved overlay import: ${from} -> ${specifier}`);
	}
	return found ?? null;
}

function moduleAt(path: string): Module {
	const raw = sources[path];
	// Preserve offsets so diagnostics still name the source line in a Svelte file.
	let source = raw;
	if (path.endsWith('.svelte')) {
		const chars = [...raw].map((char) => char === '\n' ? '\n' : ' ');
		for (const match of raw.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)) {
			const start = match.index! + match[0].indexOf('>') + 1;
			for (let i = 0; i < match[1].length; i++) chars[start + i] = match[1][i];
		}
		source = chars.join('');
	}
	const ast = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
	const mod: Module = { ast, edges: [], bindings: new Map(), lexical: new Set() };
	for (const statement of ast.statements) {
		if (ts.isVariableStatement(statement) && (statement.declarationList.flags & ts.NodeFlags.BlockScoped)) {
			for (const declaration of statement.declarationList.declarations) {
				if (ts.isIdentifier(declaration.name)) mod.lexical.add(declaration.name.text);
			}
		}
		if (ts.isImportDeclaration(statement) && ts.isStringLiteral(statement.moduleSpecifier)) {
			const clause = statement.importClause;
			if (clause?.isTypeOnly) continue;
			const named = clause?.namedBindings;
			if (named && ts.isNamedImports(named) && !clause?.name && named.elements.every((el) => el.isTypeOnly)) continue;
			const target = resolve(path, statement.moduleSpecifier.text);
			if (!target) continue;
			mod.edges.push(target);
			if (clause?.name) mod.bindings.set(clause.name.text, { target, exported: 'default' });
			if (named && ts.isNamedImports(named)) {
				for (const binding of named.elements) {
					if (!binding.isTypeOnly) mod.bindings.set(binding.name.text, {
						target, exported: (binding.propertyName ?? binding.name).text
					});
				}
			}
		}
		if (ts.isExportDeclaration(statement) && !statement.isTypeOnly && statement.moduleSpecifier && ts.isStringLiteral(statement.moduleSpecifier)) {
			if (statement.exportClause && ts.isNamedExports(statement.exportClause) && statement.exportClause.elements.every((el) => el.isTypeOnly)) continue;
			const target = resolve(path, statement.moduleSpecifier.text);
			if (target) mod.edges.push(target);
		}
	}
	return mod;
}

function routeHazards(entry: string): string[] {
	const graph = new Map<string, Module>();
	function load(path: string): void {
		if (graph.has(path)) return;
		const mod = moduleAt(path);
		graph.set(path, mod);
		for (const edge of mod.edges) load(edge);
	}
	load(entry);
	const api = '/src/lib/api.ts';
	expect(graph.has(api), `${entry} must reach the real API through static runtime imports`).toBe(true);
	function pathTo(from: string, to: string, seen = new Set<string>()): string[] | null {
		if (from === to) return [from];
		if (seen.has(from)) return null;
		seen.add(from);
		for (const edge of graph.get(from)!.edges) {
			const rest = pathTo(edge, to, seen);
			if (rest) return [from, ...rest];
		}
		return null;
	}
	const cycle = new Set([...graph.keys()].filter((path) => pathTo(api, path) && pathTo(path, api)));
	const hazards: string[] = [];
	for (const path of cycle) {
		const mod = graph.get(path)!;
		function visit(node: ts.Node): void {
			// Function bodies, type annotations and deferred imports do not execute during linking.
			if (ts.isFunctionLike(node) || ts.isTypeNode(node) || ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) return;
			if (ts.isIdentifier(node)) {
				const parent = node.parent;
				const isName = (ts.isPropertyAccessExpression(parent) && parent.name === node)
					|| (ts.isPropertyAssignment(parent) && parent.name === node)
					|| (ts.isVariableDeclaration(parent) && parent.name === node);
				const binding = mod.bindings.get(node.text);
				if (!isName && binding && cycle.has(binding.target) && graph.get(binding.target)!.lexical.has(binding.exported)) {
					const line = mod.ast.getLineAndCharacterOfPosition(node.getStart()).line + 1;
					const loop = [path, ...pathTo(binding.target, path)!];
					hazards.push(`${entry}: ${path}:${line} top-level read ${node.text}; cycle ${loop.join(' -> ')}`);
				}
			}
			ts.forEachChild(node, visit);
		}
		visit(mod.ast);
	}
	return hazards;
}

it('keep:regression overlay startup has no eager lexical reads across the API evaluation cycle', () => {
	const hazards = ['temple', 'lab'].flatMap((route) =>
		routeHazards(`/src/routes/overlay/${route}/+page.svelte`));
	expect(hazards, hazards.join('\n')).toEqual([]);
});
