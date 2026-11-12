<script lang="ts">
	import { getContext, untrack } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { invoke } from '@tauri-apps/api/core';
	import type { NavState } from '$lib/compass/navigation';
	import { LAB_SESSION, type LabNavEvent, type LabSession } from '$lib/compass/lab-session.svelte';
	import {
		createTimerState,
		formatTimer,
		startTimer as timerStart_fn,
		stopTimer as timerStop_fn,
		resetTimer as timerReset_fn,
	} from '$lib/compass/timer';

	let { configMode, label }: { configMode: boolean; label: string } = $props();

	// --- Navigation state (needed to know when we're in the lab and which room) ---
	// Display only: run measurement and submission live in $lib/run-recorder.ts,
	// which runs in the main window regardless of this overlay's toggle.
	const session = getContext<LabSession>(LAB_SESSION);

	// --- Timer state ---
	let timer = $state(createTimerState());
	let elapsed = $state(0);
	let timerText = $derived(formatTimer(elapsed));
	let hidden = $state(true);

	// --- Appearance (configurable from settings) ---
	let bgOpacity = $state(0.75);
	let textStroke = $state(true);
	let bgStyle = $derived(`rgba(13, 13, 21, ${bgOpacity})`);
	const drawing = $derived(!hidden && session.navState.inLab);

	function startTimer() { timer = timerStart_fn(timer, (e) => { elapsed = e; }); }
	function stopTimer() { timer = timerStop_fn(timer); }
	function resetTimer() { timer = timerReset_fn(timer); elapsed = 0; }

	// --- Event handling ---
	function onNavEvent(event: LabNavEvent, navAfter: NavState) {
		switch (event.type) {
			case 'PlazaEntered':
				resetTimer();
				hidden = false;
				break;
			case 'RoomChanged': {
				if (navAfter.inLab && timer.startTimestamp === null) {
					startTimer();
				}
				// Hide during Izaro fights
				const room = navAfter.currentRoom ? navAfter.roomById.get(navAfter.currentRoom) : null;
				hidden = room?.name.toLowerCase() === "aspirant's trial";
				break;
			}
			case 'LabFinished':
				// Timer keeps running through looting
				hidden = false;
				break;
			case 'LabExited':
				stopTimer();
				resetTimer();
				hidden = true;
				break;
		}
	}

	$effect(() => untrack(() => session.acquire({ onNavEvent })));

	// --- Load appearance settings ---
	$effect(() => {
		invoke<any>('get_timer_appearance').then((a) => {
			if (a) {
				bgOpacity = a.bg_opacity;
				textStroke = a.text_stroke;
			}
		}).catch(() => {});
	});

	// --- Listen for appearance changes ---
	$effect(() => {
		let cancelled = false;
		const appearancePromise = listen<any>('timer-appearance-changed', (event) => {
			if (cancelled) return;
			const a = event.payload;
			if (a) {
				bgOpacity = a.bg_opacity;
				textStroke = a.text_stroke;
			}
		});
		return () => {
			cancelled = true;
			appearancePromise.then((unlisten) => unlisten());
		};
	});
</script>

<div class="timer-container" style:background={drawing ? bgStyle : 'transparent'}>
	{#if drawing}
		<!-- The window no longer hides with the lab, so `drawing` is true only while
		     navState.inLab is true (set by PlazaEntered, cleared by LabExited, and
		     rebuilt by catch-up replay); hidden remains the in-lab rule for Izaro fights. -->
		<svg viewBox="0 0 236 60" class="timer-svg" preserveAspectRatio="xMidYMid meet">
			<text x="118" y="52" text-anchor="middle" class="timer-text" class:stroked={textStroke}>{timerText}</text>
		</svg>
	{:else if configMode}
		<p class="placeholder">{label}</p>
	{/if}
</div>

<style>
	.timer-container {
		width: 100%;
		height: 100%;
		pointer-events: none;
		border-radius: 4px;
	}

	.timer-svg {
		width: 100%;
		height: 100%;
	}

	.timer-text {
		font-family: 'Consolas', 'Monaco', monospace;
		font-size: 72px;
		font-weight: 700;
		fill: #e5e7eb;
		letter-spacing: 0.05em;
	}

	.timer-text.stroked {
		stroke: rgba(0, 0, 0, 0.85);
		stroke-width: 2px;
		paint-order: stroke fill;
	}

	.placeholder {
		padding: 4px 8px;
		font-size: 11px;
		color: var(--color-lab-text-muted);
	}
</style>
