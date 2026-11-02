/** Read a decimal input independently of the webview's locale. Blank is invalid. */
export function parseDecimalInput(raw: string): number {
	const decimal = raw.trim().replace(',', '.');
	if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(decimal)) return NaN;
	const value = Number(decimal);
	return Number.isFinite(value) ? value : NaN;
}
