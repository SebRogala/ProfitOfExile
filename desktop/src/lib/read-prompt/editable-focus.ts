export function reportEditableFocus(report: (editable: boolean) => void): () => void {
	let stopped = false;
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
		report(editable);
	};
	const focusout = () => queueMicrotask(update);
	const blur = () => report(false);
	document.addEventListener('focusin', update);
	document.addEventListener('focusout', focusout);
	window.addEventListener('focus', update);
	window.addEventListener('blur', blur);
	update();
	return () => {
		stopped = true;
		document.removeEventListener('focusin', update);
		document.removeEventListener('focusout', focusout);
		window.removeEventListener('focus', update);
		window.removeEventListener('blur', blur);
		report(false);
	};
}
