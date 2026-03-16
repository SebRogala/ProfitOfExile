<script lang="ts">
	let { data, width = 80, height = 20 }: { data: number[]; width?: number; height?: number } =
		$props();

	function computePath(values: number[], w: number, h: number): string {
		if (!values.length) return '';
		const max = Math.max(...values);
		const min = Math.min(...values);
		// Ensure range is at least 20% of the midpoint so small variations don't look extreme
		const mid = (max + min) / 2 || 1;
		const range = Math.max(max - min, mid * 0.2) || 1;
		const step = w / (values.length - 1 || 1);
		return values
			.map((v, i) => {
				const x = i * step;
				const y = h - ((v - min) / range) * (h - 2) - 1;
				return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`;
			})
			.join(' ');
	}

	let path = $derived(computePath(data, width, height));
</script>

<svg {width} {height} viewBox="0 0 {width} {height}" class="sparkline">
	<path d={path} fill="none" stroke="var(--color-lab-blue)" stroke-width="1.5" />
</svg>

<style>
	.sparkline {
		display: inline-block;
		vertical-align: middle;
	}
</style>
