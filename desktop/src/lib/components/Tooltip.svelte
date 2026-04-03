<script lang="ts">
	let { text, children, position = 'above' }: { text: string; children: any; position?: 'above' | 'below' } = $props();

	let wrap = $state<HTMLElement | null>(null);
	let tipEl: HTMLDivElement | null = null;
	let visible = $state(false);
	let hideTimeout: ReturnType<typeof setTimeout> | null = null;

	function ensureTip(): HTMLDivElement {
		if (!tipEl) {
			tipEl = document.createElement('div');
			tipEl.style.cssText = `
				position: fixed;
				z-index: 9999;
				background: #1a1d27;
				border: 1px solid #2a2d37;
				padding: 8px 12px;
				max-width: 400px;
				font-size: 0.75rem;
				font-weight: 400;
				line-height: 1.5;
				color: #e4e4e7;
				box-shadow: 0 8px 32px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.08);
				white-space: normal;
				pointer-events: auto;
				opacity: 0;
				visibility: hidden;
				font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
			`;
			tipEl.addEventListener('mouseenter', () => {
				if (hideTimeout) { clearTimeout(hideTimeout); hideTimeout = null; }
			});
			tipEl.addEventListener('mouseleave', () => {
				hide();
			});
			document.body.appendChild(tipEl);
		}
		return tipEl;
	}

	function show() {
		if (hideTimeout) { clearTimeout(hideTimeout); hideTimeout = null; }
		visible = true;
		const tip = ensureTip();
		// text is developer-authored tooltip content (hardcoded strings from tooltips.ts),
		// not user input — safe to render as HTML for styled signal descriptions.
		tip.innerHTML = text; // eslint-disable-line no-unsanitized/property
		tip.style.opacity = '1';
		tip.style.visibility = 'visible';
		requestAnimationFrame(() => positionTip());
	}

	function hide() {
		if (hideTimeout) clearTimeout(hideTimeout);
		hideTimeout = setTimeout(() => {
			visible = false;
			if (tipEl) {
				tipEl.style.opacity = '0';
				tipEl.style.visibility = 'hidden';
			}
			hideTimeout = null;
		}, 100);
	}

	function positionTip() {
		if (!wrap || !tipEl) return;
		const rect = wrap.getBoundingClientRect();
		const tipRect = tipEl.getBoundingClientRect();
		let top: number;
		let left = rect.left;

		if (position === 'below') {
			top = rect.bottom + 4;
			// Flip above if no room below
			if (top + tipRect.height > window.innerHeight - 8) {
				top = rect.top - 4 - tipRect.height;
			}
		} else {
			top = rect.top - 4 - tipRect.height;
			// Flip below if no room above
			if (top < 8) top = rect.bottom + 4;
		}

		if (left + tipRect.width > window.innerWidth - 8) {
			left = window.innerWidth - tipRect.width - 8;
		}
		if (left < 8) left = 8;

		tipEl.style.top = `${top}px`;
		tipEl.style.left = `${left}px`;
	}

	$effect(() => {
		return () => {
			if (tipEl) {
				document.body.removeChild(tipEl);
				tipEl = null;
			}
		};
	});
</script>

<span class="tooltip-wrap" role="tooltip" bind:this={wrap} onmouseenter={show} onmouseleave={hide}>
	{@render children()}
</span>

<style>
	.tooltip-wrap {
		cursor: help;
	}
</style>
