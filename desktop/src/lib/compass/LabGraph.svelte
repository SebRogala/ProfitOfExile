<script lang="ts">
	import type { NavState } from './navigation';
	import type { LabLayoutRoom } from './navigation';

	let {
		navState,
		width = 800,
		height = 300,
		padding = 30,
		nodeRadius = 14,
		showLabels = true,
		showContentDots = true,
		compact = false,
		currentRoomId = null as string | null,
		visitedRoomIds = null as string[] | null,
		editingRoomId = null as string | null,
		interactive = false,
		onRoomClick = null as ((roomId: string) => void) | null,
		onRoomRightClick = null as ((roomId: string) => void) | null,
	} = $props();

	// --- Direction angle mapping for exit dots on circle perimeter ---
	const DIR_ANGLE: Record<string, number> = {
		N: -90, NE: -45, E: 0, SE: 45, S: 90, SW: 135, W: 180, NW: -135, C: -90,
	};

	// --- Content info ---
	interface ContentInfo { color: string; symbol: string; }

	function getContentInfoList(contents: string[]): ContentInfo[] {
		const result: ContentInfo[] = [];
		const cl = contents.map(c => c.toLowerCase());
		if (cl.includes('darkshrine')) result.push({ color: '#ef4444', symbol: 'D' });
		if (cl.includes('argus')) result.push({ color: '#f59e0b', symbol: 'A' });
		if (cl.some(c => c.includes('golden-key'))) result.push({ color: '#fbbf24', symbol: 'K' });
		if (cl.some(c => c.includes('golden-door'))) result.push({ color: '#f59e0b', symbol: 'GD' });
		if (cl.some(c => c.includes('gauntlet'))) result.push({ color: '#60a5fa', symbol: 'G' });
		if (cl.some(c => c.includes('puzzle'))) result.push({ color: '#60a5fa', symbol: 'P' });
		if (cl.some(c => c.includes('silver-key'))) result.push({ color: '#9ca3af', symbol: 'k' });
		if (cl.some(c => c.includes('silver-door'))) result.push({ color: '#9ca3af', symbol: 'd' });
		return result;
	}

	// --- Room nodes ---
	interface RoomNode {
		room: LabLayoutRoom;
		cx: number; cy: number;
		onRoute: boolean;
		isTrial: boolean;
		contents: ContentInfo[];
		isCurrent: boolean;
		isVisited: boolean;
		exitDirs: string[];
	}

	interface Edge {
		x1: number; y1: number; x2: number; y2: number;
		onRoute: boolean; isVisited: boolean; isSecret: boolean;
		key: string;
	}

	let roomNodes = $derived.by<RoomNode[]>(() => {
		if (!navState.layout) return [];
		const rooms = navState.layout.rooms;
		if (rooms.length === 0) return [];

		const xs = rooms.map((r: LabLayoutRoom) => parseFloat(r.x));
		const ys = rooms.map((r: LabLayoutRoom) => parseFloat(r.y));
		const minX = Math.min(...xs), maxX = Math.max(...xs);
		const minY = Math.min(...ys), maxY = Math.max(...ys);
		const rangeX = maxX - minX || 1;
		const rangeY = maxY - minY || 1;

		const routeSet = new Set(navState.plannedRoute);
		const visitedSet = new Set<string>();
		if (visitedRoomIds) {
			// Explicit visited list provided (from overlay tracking)
			for (const id of visitedRoomIds) visitedSet.add(id);
		} else if (currentRoomId && navState.plannedRoute.length > 0) {
			// Fallback: infer from route position (for planner)
			const currentIdx = navState.plannedRoute.indexOf(currentRoomId);
			if (currentIdx > 0) {
				for (let i = 0; i < currentIdx; i++) visitedSet.add(navState.plannedRoute[i]);
			}
		}

		// Find the median Y to use as the "main path" baseline
		const sortedYs = [...ys].sort((a, b) => a - b);
		const medianY = sortedYs[Math.floor(sortedYs.length / 2)];

		return rooms.map((room: LabLayoutRoom): RoomNode => {
			const rawX = parseFloat(room.x), rawY = parseFloat(room.y);
			const cx = padding + ((rawX - minX) / rangeX) * (width - 2 * padding);
			// Compress vertical distance: branch rooms pulled closer to main path
			const yOff = rawY - medianY;
			const compressedY = medianY + yOff * 0.55; // 55% of original distance
			const cy = padding + ((compressedY - minY) / rangeY) * (height - 2 * padding);
			const isTrial = room.name.toLowerCase() === "aspirant's trial";

			return {
				room, cx, cy,
				onRoute: routeSet.has(room.id),
				isTrial,
				contents: getContentInfoList(room.contents),
				isCurrent: room.id === currentRoomId,
				isVisited: visitedSet.has(room.id),
				exitDirs: Object.keys(room.exits),
			};
		});
	});

	// Directed connections: from exit dot on source room → target room perimeter
	interface Connection {
		// Start: exit dot position on source room perimeter
		x1: number; y1: number;
		// End: target room perimeter (clipped)
		x2: number; y2: number;
		onRoute: boolean;
		isVisited: boolean;
		isSecret: boolean;
		key: string;
	}

	let connections = $derived.by<Connection[]>(() => {
		if (!navState.layout || roomNodes.length === 0) return [];
		const nodeMap = new Map<string, RoomNode>();
		for (const node of roomNodes) nodeMap.set(node.room.id, node);

		const routeEdges = new Set<string>();
		const visitedEdges = new Set<string>();
		const visitedNodeSet = visitedRoomIds ? new Set(visitedRoomIds) : null;

		for (let i = 0; i < navState.plannedRoute.length - 1; i++) {
			const pair = [navState.plannedRoute[i], navState.plannedRoute[i + 1]].sort().join('|');
			routeEdges.add(pair);
			// Edge is visited if both endpoints are visited
			if (visitedNodeSet) {
				if (visitedNodeSet.has(navState.plannedRoute[i]) && visitedNodeSet.has(navState.plannedRoute[i + 1])) {
					visitedEdges.add(pair);
				}
			} else {
				const currentIdx = currentRoomId ? navState.plannedRoute.indexOf(currentRoomId) : -1;
				if (currentIdx > 0 && i < currentIdx) visitedEdges.add(pair);
			}
		}

		const seen = new Set<string>();
		const result: Connection[] = [];

		for (const room of navState.layout.rooms) {
			const fromNode = nodeMap.get(room.id);
			if (!fromNode) continue;
			const fromR = nodeR(fromNode);

			for (const [dir, targetId] of Object.entries(room.exits)) {
				const toNode = nodeMap.get(targetId as string);
				if (!toNode) continue;

				const isSecret = dir === 'C';
				// Dedup per pair AND per flavor: a natural exit (e.g. SE) and a
				// secret passage (C) between the SAME two rooms are two distinct
				// connections and must both be rendered — one solid, one dashed.
				const pairKey = [room.id, targetId].sort().join('|');
				const dedupKey = pairKey + (isSecret ? ':secret' : ':natural');
				if (seen.has(dedupKey)) continue;
				seen.add(dedupKey);

				const toR = nodeR(toNode);

				// Izaro trials + secret passages: center-to-center (clipped)
				// Regular rooms: exit dot → target room perimeter
				let startX: number, startY: number;
				if (fromNode.isTrial || isSecret) {
					const dx0 = toNode.cx - fromNode.cx, dy0 = toNode.cy - fromNode.cy;
					const d0 = Math.sqrt(dx0 * dx0 + dy0 * dy0);
					startX = d0 > 0 ? fromNode.cx + (dx0 / d0) * (fromR + 2) : fromNode.cx;
					startY = d0 > 0 ? fromNode.cy + (dy0 / d0) * (fromR + 2) : fromNode.cy;
				} else {
					const exitDot = exitDotPos(fromNode.cx, fromNode.cy, dir, fromR);
					startX = exitDot.x;
					startY = exitDot.y;
				}

				// End: clip to target room perimeter
				const dx = toNode.cx - startX, dy = toNode.cy - startY;
				const dist = Math.sqrt(dx * dx + dy * dy);
				const endX = dist > 0 ? toNode.cx - (dx / dist) * (toR + 2) : toNode.cx;
				const endY = dist > 0 ? toNode.cy - (dy / dist) * (toR + 2) : toNode.cy;

				result.push({
					x1: startX, y1: startY,
					x2: endX, y2: endY,
					onRoute: routeEdges.has(pairKey),
					isVisited: visitedEdges.has(pairKey),
					isSecret,
					key: dedupKey,
				});
			}
		}
		return result;
	});

	const r = $derived(compact ? nodeRadius * 0.7 : nodeRadius);
	const trialScale = 1.35;

	function exitDotPos(cx: number, cy: number, dir: string, radius: number): { x: number; y: number } {
		const angle = (DIR_ANGLE[dir] ?? 0) * Math.PI / 180;
		return { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) };
	}

	function nodeR(node: RoomNode): number {
		return node.isTrial ? r * trialScale : r;
	}
</script>

<svg
	viewBox="0 0 {width} {height}"
	class="lab-graph"
	class:compact
	xmlns="http://www.w3.org/2000/svg"
>
	<!-- Layer 1: Room nodes -->
	{#each roomNodes as node (node.room.id)}
		{@const nr = nodeR(node)}
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<g
			class="room-group"
			class:off-route={!node.onRoute}
			class:is-current={node.isCurrent}
			class:is-visited={node.isVisited}
			role={interactive ? 'button' : undefined}
			tabindex={interactive ? 0 : undefined}
			onclick={() => interactive && onRoomClick?.(node.room.id)}
			onkeydown={(e) => { if (interactive && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); onRoomClick?.(node.room.id); } }}
			oncontextmenu={(e) => { if (onRoomRightClick) { e.preventDefault(); onRoomRightClick(node.room.id); } }}
			style={interactive ? 'cursor: pointer;' : ''}
		>
			{#if node.isCurrent}
				<!-- Dark outline behind current room so green circle pops against green route lines -->
				<circle cx={node.cx} cy={node.cy} r={nr + 2}
					fill="none" stroke="#0f172a" stroke-width="2" />
			{/if}
			<circle
				cx={node.cx} cy={node.cy} r={nr}
				class="room-circle"
				class:room-trial={node.isTrial}
				class:room-target={navState.targetRooms.includes(node.room.id)}
				class:room-current={node.isCurrent}
				class:room-editing={node.room.id === editingRoomId}
			/>
			{#if node.isTrial}
				<!-- Izaro: inner ring for ornate look -->
				<circle cx={node.cx} cy={node.cy} r={nr * 0.75}
					fill="none" stroke="#f59e0b" stroke-width="1" opacity="0.5" />
			{/if}

			<!-- Room ID inside circle -->
			{#if !compact && !node.isTrial}
				<text x={node.cx} y={node.cy + nr * 0.3} class="room-id"
					font-size={nr * 0.55}>{node.room.id}</text>
			{/if}

			<!-- Exit dots rendered in Layer 4 (topmost) -->

			<!-- Izaro trial label -->
			{#if node.isTrial}
				<text x={node.cx} y={node.cy + nr * 0.35} class="trial-label" font-size={nr * 0.6}>I</text>
			{/if}

			<!-- Golden key/door/argus inside circle -->
			{#if !node.isVisited && !node.isTrial}
				{@const hasKey = node.contents.some(c => c.symbol === 'K')}
				{@const hasDoor = node.contents.some(c => c.symbol === 'GD')}
				{@const hasArgus = node.contents.some(c => c.symbol === 'A')}
				{#if hasKey && hasDoor}
					<!-- Both key + door: split left/right -->
					<defs>
						<clipPath id="clip-l-{node.room.id}">
							<rect x={node.cx - nr} y={node.cy - nr} width={nr} height={nr * 2} />
						</clipPath>
						<clipPath id="clip-r-{node.room.id}">
							<rect x={node.cx} y={node.cy - nr} width={nr} height={nr * 2} />
						</clipPath>
					</defs>
					<circle cx={node.cx} cy={node.cy} r={nr - 3}
						fill="#fbbf24" opacity="0.9" clip-path="url(#clip-l-{node.room.id})" />
					<circle cx={node.cx} cy={node.cy} r={nr - 3}
						fill="#d97706" opacity="0.9" clip-path="url(#clip-r-{node.room.id})" />
					{#if !compact}
						<text x={node.cx - nr * 0.25} y={node.cy + nr * 0.25} fill="#1c1917"
							font-size={nr * 0.5} text-anchor="middle" font-weight="900"
							pointer-events="none">K</text>
						<text x={node.cx + nr * 0.25} y={node.cy + nr * 0.25} fill="#1c1917"
							font-size={nr * 0.5} text-anchor="middle" font-weight="900"
							pointer-events="none">D</text>
					{/if}
				{:else if hasKey}
					<circle cx={node.cx} cy={node.cy} r={nr - 3} fill="#fbbf24" opacity="0.9" />
					<text x={node.cx} y={node.cy + nr * 0.25} fill="#1c1917"
						font-size={nr * 0.7} text-anchor="middle" font-weight="900"
						pointer-events="none">K</text>
				{:else if hasDoor}
					<circle cx={node.cx} cy={node.cy} r={nr - 3} fill="#d97706" opacity="0.9" />
					<text x={node.cx} y={node.cy + nr * 0.25} fill="#1c1917"
						font-size={nr * 0.7} text-anchor="middle" font-weight="900"
						pointer-events="none">D</text>
				{:else if hasArgus}
					<circle cx={node.cx} cy={node.cy} r={nr - 3} fill="#ea580c" opacity="0.9" />
					<text x={node.cx} y={node.cy + nr * 0.25} fill="#fff"
						font-size={nr * 0.7} text-anchor="middle" font-weight="900"
						pointer-events="none">A</text>
				{/if}
			{/if}

			<!-- Other content badges BELOW room -->
			{#if showContentDots && !node.isVisited}
				{@const badges = node.contents.filter(c => !['K', 'GD', 'A'].includes(c.symbol))}
				{#if badges.length > 0}
					{#each badges as ci, idx}
						{@const badgeW = compact ? 8 : 20}
						{@const badgeH = compact ? 6 : 14}
						{@const gap = compact ? 2 : 3}
						{@const totalW = badges.length * (badgeW + gap) - gap}
						{@const bx = node.cx - totalW / 2 + idx * (badgeW + gap) + badgeW / 2}
						{@const by = node.cy + nr + (compact ? 4 : 8) + badgeH / 2}
						<rect
							x={bx - badgeW / 2} y={by - badgeH / 2}
							width={badgeW} height={badgeH} rx="3"
							fill={ci.color}
						/>
						{#if !compact}
							<text x={bx} y={by + badgeH * 0.25} fill="#fff"
								font-size={badgeH * 0.8} text-anchor="middle"
								font-weight="800" pointer-events="none">{ci.symbol}</text>
						{/if}
					{/each}
				{/if}
			{/if}

			<title>{node.room.name}{node.room.contents.length > 0 ? '\n' + node.room.contents.join(', ') : ''}{node.room.secret_passage ? '\nSecret: ' + node.room.secret_passage : ''}</title>
		</g>
	{/each}

	<!-- Layer 2: Non-route connections (on top of rooms, visible) -->
	{#each connections.filter(c => !c.onRoute) as conn (conn.key + '-off')}
		<line
			x1={conn.x1} y1={conn.y1} x2={conn.x2} y2={conn.y2}
			class="conn-off"
			class:edge-visited={conn.isVisited}
			class:edge-secret={conn.isSecret}
		/>
	{/each}

	<!-- Layer 3: Route connections -->
	{#each connections.filter(c => c.onRoute && !c.isVisited) as conn (conn.key + '-route')}
		<line
			x1={conn.x1} y1={conn.y1} x2={conn.x2} y2={conn.y2}
			class="conn-route"
			class:edge-secret={conn.isSecret}
		/>
	{/each}

	<!-- Layer 4: Exit dots (topmost — always visible, even over route lines) -->
	{#each roomNodes as node (node.room.id + '-dots')}
		{@const nr = nodeR(node)}
		{#if !node.isTrial}
			{#each Object.entries(node.room.exits) as [dir, targetId]}
				{#if dir !== 'C'}
					{@const dot = exitDotPos(node.cx, node.cy, dir, nr)}
					{@const pairKey = [node.room.id, targetId].sort().join('|')}
					{@const isRouteExit = connections.some(c => c.key === pairKey && c.onRoute)}
					<circle cx={dot.x} cy={dot.y} r={compact ? 2 : (isRouteExit ? 5 : 4.5)}
						fill={isRouteExit ? '#10b981' : '#94a3b8'}
						stroke={isRouteExit ? '#059669' : '#64748b'}
						stroke-width={compact ? 0.5 : 1}
						opacity={node.isVisited ? 0.2 : 1} />
				{/if}
			{/each}
		{/if}
	{/each}
</svg>

<style>
	.lab-graph {
		width: 100%;
		height: 100%;
		display: block;
	}

	.conn-off {
		stroke: #94a3b8;
		stroke-width: 2;
		stroke-linecap: round;
		opacity: 0.6;
	}

	.compact .conn-off {
		stroke-width: 1;
		opacity: 0.4;
	}

	.conn-route {
		stroke: #10b981;
		stroke-width: 4;
		stroke-linecap: round;
	}

	.compact .conn-route {
		stroke-width: 2;
	}

	.edge-visited {
		opacity: 0.2;
	}

	.edge-secret {
		stroke-dasharray: 6, 3;
		stroke: #a78bfa;
		stroke-width: 2.5;
	}

	.edge-secret.edge-visited {
		stroke: #4b5563;
		opacity: 0.15;
	}

	.room-group {
		transition: opacity 0.2s;
	}

	.room-group.off-route {
		opacity: 0.35;
	}

	.room-group.is-visited {
		opacity: 0.3;
	}

	.room-group.is-current {
		opacity: 1;
	}

	.room-circle {
		fill: #1f2937;
		stroke: #6b7280;
		stroke-width: 2.5;
	}

	.compact .room-circle {
		stroke-width: 1.5;
	}

	.room-circle.room-trial {
		stroke: #f59e0b;
		stroke-width: 3;
		fill: #1c1917;
	}

	.room-circle.room-target {
		stroke: #10b981;
		stroke-width: 2.5;
	}

	.room-circle.room-current {
		stroke: #22c55e;
		stroke-width: 3;
		fill: #064e3b;
	}

	.room-circle.room-editing {
		stroke: #f43f5e;
		stroke-width: 4;
		filter: drop-shadow(0 0 8px rgba(244, 63, 94, 0.7));
	}

	.room-id {
		fill: #d1d5db;
		text-anchor: middle;
		font-weight: 700;
		pointer-events: none;
	}

	.trial-label {
		fill: #f59e0b;
		text-anchor: middle;
		font-weight: 700;
		pointer-events: none;
	}

</style>
