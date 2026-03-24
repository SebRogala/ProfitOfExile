<script lang="ts">
	let {
		data,
		hourlyMedians,
		todayHourMedians,
		height = 140,
	}: {
		data: { time: string; price: number }[];
		hourlyMedians?: { hour: number; median: number }[];
		todayHourMedians?: { hour: number; median: number }[];
		height?: number;
	} = $props();

	const PAD_LEFT = 52;
	const PAD_RIGHT = 12;
	const PAD_TOP = 16;
	const PAD_BOTTOM = 28;

	let containerEl = $state<HTMLDivElement | null>(null);
	let chartWidth = $state(600);

	$effect(() => {
		if (containerEl) {
			const obs = new ResizeObserver(entries => {
				chartWidth = entries[0].contentRect.width;
			});
			obs.observe(containerEl);
			return () => obs.disconnect();
		}
	});

	let plotW = $derived(chartWidth - PAD_LEFT - PAD_RIGHT);
	let plotH = $derived(height - PAD_TOP - PAD_BOTTOM);

	// --- Nice tick computation ---
	function niceStep(rawRange: number, targetTicks: number): number {
		const rough = rawRange / targetTicks;
		const mag = Math.pow(10, Math.floor(Math.log10(rough)));
		const norm = rough / mag;
		let nice: number;
		if (norm <= 1.5) nice = 1;
		else if (norm <= 3) nice = 2;
		else if (norm <= 7) nice = 5;
		else nice = 10;
		return nice * mag;
	}

	let prices = $derived(data.map(d => d.price));
	let dataMin = $derived(prices.length ? Math.min(...prices) : 0);
	let dataMax = $derived(prices.length ? Math.max(...prices) : 0);
	let rawRange = $derived(dataMax - dataMin || dataMax * 0.1 || 1);

	let step = $derived(niceStep(rawRange, 3));
	// Extend to nice boundaries + one extra step for breathing.
	let minP = $derived(Math.floor(dataMin / step) * step - step * 0.5);
	let maxP = $derived(Math.ceil(dataMax / step) * step + step * 0.5);
	let rangeP = $derived(maxP - minP || 1);

	// Y-axis ticks at nice intervals.
	let yTicks = $derived.by(() => {
		const ticks: { y: number; text: string }[] = [];
		let v = Math.ceil(minP / step) * step;
		while (v <= maxP) {
			ticks.push({ y: yFromPrice(v), text: `${Math.round(v)}c` });
			v += step;
		}
		return ticks;
	});

	// Time range: data + ~12% future for prediction.
	let firstMs = $derived(data.length ? new Date(data[0].time).getTime() : 0);
	let lastMs = $derived(data.length ? new Date(data[data.length - 1].time).getTime() : 0);
	let dataRangeMs = $derived(lastMs - firstMs || 1);
	let futureMs = $derived(dataRangeMs * 0.12);
	let totalRangeMs = $derived(dataRangeMs + futureMs);

	function xFromMs(ms: number): number {
		return PAD_LEFT + ((ms - firstMs) / totalRangeMs) * plotW;
	}
	function yFromPrice(price: number): number {
		return PAD_TOP + plotH - ((price - minP) / rangeP) * plotH;
	}

	// History line.
	let linePath = $derived.by(() => {
		if (data.length < 2) return '';
		return data.map((d, i) => {
			const px = xFromMs(new Date(d.time).getTime());
			const py = yFromPrice(d.price);
			return `${i === 0 ? 'M' : 'L'}${px.toFixed(1)},${py.toFixed(1)}`;
		}).join(' ');
	});

	// "Now" marker.
	let nowX = $derived.by(() => {
		if (data.length < 2) return null;
		const nx = xFromMs(Date.now());
		if (nx < PAD_LEFT || nx > PAD_LEFT + plotW) return null;
		return nx;
	});

	// Prediction line: apply historical hour-to-hour relative changes to current price.
	// Prefers today's weekday-specific medians (e.g., "what does Monday look like")
	// and falls back to overall medians for hours without weekday-specific data.
	let predictionPath = $derived.by(() => {
		const baseMedians = hourlyMedians?.length ? hourlyMedians : [];
		const todayMedians = todayHourMedians?.length ? todayHourMedians : [];
		if (!baseMedians.length || data.length < 2) return '';

		// Today's weekday medians take priority, fall back to overall.
		const overallByHour = new Map(baseMedians.map(h => [h.hour, h.median]));
		const todayByHour = new Map(todayMedians.map(h => [h.hour, h.median]));
		const medianByHour = new Map([...overallByHour, ...todayByHour]); // today overwrites overall

		const lastPt = data[data.length - 1];
		const lastTime = new Date(lastPt.time).getTime();
		const endTime = firstMs + totalRangeMs;

		const points: string[] = [];
		points.push(`M${xFromMs(lastTime).toFixed(1)},${yFromPrice(lastPt.price).toFixed(1)}`);

		let predicted = lastPt.price;
		let prevHour = new Date(lastPt.time).getUTCHours();
		let t = lastTime + 3600000;
		while (t <= endTime) {
			const h = new Date(t).getUTCHours();
			const currMedian = medianByHour.get(h);
			const prevMedian = medianByHour.get(prevHour);
			if (currMedian != null && prevMedian != null && prevMedian > 0) {
				// Apply the historical relative change between these hours.
				const change = currMedian / prevMedian;
				predicted = predicted * change;
			}
			points.push(`L${xFromMs(t).toFixed(1)},${yFromPrice(predicted).toFixed(1)}`);
			prevHour = h;
			t += 3600000;
		}
		return points.length > 1 ? points.join(' ') : '';
	});

	// X-axis labels every 3 hours.
	let xLabels = $derived.by(() => {
		if (data.length < 2) return [];
		const labels: { x: number; text: string; bold: boolean }[] = [];
		let lastX = -Infinity;
		const endMs = firstMs + totalRangeMs;
		const startDt = new Date(firstMs);
		startDt.setUTCMinutes(0, 0, 0);
		let h = startDt.getUTCHours();
		h = Math.ceil(h / 3) * 3;
		startDt.setUTCHours(h);
		let t = startDt.getTime();
		while (t <= endMs) {
			const px = xFromMs(t);
			if (px >= PAD_LEFT && (px - lastX) > 36) {
				const dt = new Date(t);
				const hour = dt.getUTCHours();
				const isDayBoundary = hour === 0;
				const text = isDayBoundary
					? dt.toLocaleDateString(undefined, { weekday: 'short', timeZone: 'UTC' })
					: `${String(hour).padStart(2, '0')}:00`;
				labels.push({ x: px, text, bold: isDayBoundary });
				lastX = px;
			}
			t += 3 * 3600000;
		}
		return labels;
	});
</script>

<div class="chart-container" bind:this={containerEl}>
	<svg width={chartWidth} {height} viewBox="0 0 {chartWidth} {height}">
		<!-- Grid lines -->
		{#each yTicks as yt}
			<line x1={PAD_LEFT} x2={chartWidth - PAD_RIGHT} y1={yt.y} y2={yt.y} stroke="rgba(255,255,255,0.05)" stroke-width="1" />
		{/each}

		<!-- Future zone background -->
		{#if nowX}
			<rect x={nowX} y={PAD_TOP} width={chartWidth - PAD_RIGHT - nowX} height={plotH} fill="rgba(251,191,36,0.03)" />
		{/if}

		<!-- History line -->
		{#if linePath}
			<path d={linePath} fill="none" stroke="#3b82f6" stroke-width="2" stroke-linejoin="round" />
		{/if}

		<!-- Prediction line -->
		{#if predictionPath}
			<path d={predictionPath} fill="none" stroke="rgba(59,130,246,0.4)" stroke-width="2" stroke-dasharray="6,4" stroke-linejoin="round" />
		{/if}

		<!-- Now marker -->
		{#if nowX}
			<line x1={nowX} x2={nowX} y1={PAD_TOP} y2={height - PAD_BOTTOM} stroke="rgba(251,191,36,0.6)" stroke-width="1.5" />
			<text x={nowX} y={PAD_TOP - 4} fill="#fbbf24" font-size="10" text-anchor="middle" font-weight="600" font-family="inherit">now</text>
		{/if}

		<!-- Y labels -->
		{#each yTicks as yt}
			<text x={PAD_LEFT - 6} y={yt.y + 4} fill="#94a3b8" font-size="11" text-anchor="end" font-family="inherit">{yt.text}</text>
		{/each}

		<!-- X labels -->
		{#each xLabels as xl}
			<text x={xl.x} y={height - 6} fill={xl.bold ? '#cbd5e1' : '#64748b'} font-size={xl.bold ? '11' : '10'} font-weight={xl.bold ? '700' : '400'} text-anchor="middle" font-family="inherit">{xl.text}</text>
		{/each}
	</svg>
</div>

<style>
	.chart-container {
		width: 100%;
		padding: 4px 0;
	}
	svg {
		display: block;
	}
</style>
