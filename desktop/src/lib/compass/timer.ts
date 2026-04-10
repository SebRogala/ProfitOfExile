/**
 * Shared lab run timer — used by both the compass overlay (minimal mode)
 * and the standalone timer overlay.
 *
 * State management with interval-based ticking — no Svelte reactivity.
 * Callers store the state in $state, call these functions, and must
 * ensure stopTimer/resetTimer is called for cleanup before discarding state.
 */

export interface TimerState {
	startTimestamp: number | null;
	elapsed: number;
	interval: ReturnType<typeof setInterval> | null;
}

export function createTimerState(): TimerState {
	return { startTimestamp: null, elapsed: 0, interval: null };
}

export function formatTimer(seconds: number): string {
	const m = Math.floor(seconds / 60);
	const s = seconds % 60;
	return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

/**
 * Start the timer. Returns a new state with the interval running.
 * The `onTick` callback is called every second with the updated elapsed value.
 */
export function startTimer(state: TimerState, onTick: (elapsed: number) => void): TimerState {
	if (state.startTimestamp !== null) return state;
	// Clear any orphaned interval (defensive — callers should use resetTimer)
	if (state.interval !== null) clearInterval(state.interval);
	const startTimestamp = Date.now();
	const interval = setInterval(() => {
		onTick(Math.floor((Date.now() - startTimestamp) / 1000));
	}, 1000);
	return { startTimestamp, elapsed: 0, interval };
}

/** Stop the timer, preserving the elapsed value. */
export function stopTimer(state: TimerState): TimerState {
	if (state.interval !== null) {
		clearInterval(state.interval);
	}
	return { ...state, interval: null };
}

/** Reset the timer to zero and stop the interval. */
export function resetTimer(state: TimerState): TimerState {
	if (state.interval !== null) {
		clearInterval(state.interval);
	}
	return { startTimestamp: null, elapsed: 0, interval: null };
}
