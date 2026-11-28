export function reportEditableFocus(report: (editable: boolean, active: boolean) => void): () => void {
	let stopped = false;
	let active = document.hasFocus();
	const update = () => {
		if (stopped) return;
		let element = document.activeElement;
		let editable = false;
		while (element) {
			if (element.matches('input, textarea, select, [role="combobox"], [role="textbox"], [role="searchbox"], [role="spinbutton"], [role="listbox"]')) {
				editable = true;
				break;
			}
			const value = element.getAttribute('contenteditable');
			if (value !== null) {
				editable = value !== 'false';
				break;
			}
			element = element.parentElement;
		}
		report(editable, active);
	};
	const focusout = () => queueMicrotask(update);
	const focus = () => { active = true; update(); };
	const blur = () => { active = false; report(false, false); };
	document.addEventListener('focusin', update);
	document.addEventListener('focusout', focusout);
	window.addEventListener('focus', focus);
	window.addEventListener('blur', blur);
	update();
	return () => {
		stopped = true;
		document.removeEventListener('focusin', update);
		document.removeEventListener('focusout', focusout);
		window.removeEventListener('focus', focus);
		window.removeEventListener('blur', blur);
		report(false, false);
	};
}
