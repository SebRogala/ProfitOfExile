/* POE-317 shared fixture: one dataset for every board.
   Rates: the owner's 78-run Catarina sheet (each run has exactly one unique-pool
   drop). Wiki rates: poewiki listing supplied by the owner 2026-11-01.
   Base types confirmed against poe.ninja 2026-11-01. Seed runs carry no dates. */
window.POE317 = (function () {
	var B = '/_blob/';
	var icons = {
		urn: B + 'ad9f408c9d6c5c0f04678e30ff690583', spinehail: B + '7b30a3a9870578e59076e048a03b8728',
		diadem: B + '1440bad354d11594735ecf0c4d48df78', bitterbind: B + '7eaf40d8e92f04cb4cc436b0afcd0fd0',
		queens: B + '8240a2f0e5a464b6354da8fb191ab545', cane: B + 'b7aa20d6cc278f5631055cab7c3f16a7',
		veiledex: B + '638074112e03767077a04b15fa71de29', allflame: B + '4bdfc3257727c48049590fc1df98801b',
		communion: B + 'cd55939a180cf1c6248987883ed342e4', divcard: B + 'bf252a6d0ddd38ec200bf48b1d496b12',
		astrolabe: B + 'ef1b780ab47bf7ce3c7b879190019ad2', eye: B + '269530bff797b63137ed7dad4f982a96'
	};
	var runs = 78;
	var pool = [
		{ id: 'urn', name: 'Cinderswallow Urn', base: 'Silver Flask', drops: 24, wiki: '30%' },
		{ id: 'spinehail', name: 'Spinehail', base: 'Vile Arrow Quiver', drops: 16, wiki: '20%' },
		{ id: 'cane', name: 'Cane of Kulemak', base: 'Serpentine Staff', drops: 14, wiki: '18%' },
		{ id: 'diadem', name: 'The Devouring Diadem', base: 'Necromancer Circlet', drops: 11, wiki: '16%' },
		{ id: 'bitterbind', name: 'Bitterbind Point', base: 'Titanium Spirit Shield', drops: 6, wiki: '10%' },
		{ id: 'queens', name: "The Queen's Hunger", base: 'Vaal Regalia', drops: 7, wiki: '6%' }
	];
	var extra = [
		{ id: 'eye', name: "Zorath's Eye of the Endless", base: 'Map fragment', drops: 33, wiki: '~50%' },
		{ id: 'veiledex', name: 'Veiled Exalted Orb', base: 'Currency', drops: 26, wiki: '25%' },
		{ id: 'allflame', name: 'Allflame Ember of Kulemak', base: 'Allflame Ember', drops: 22, wiki: '60%' },
		{ id: 'communion', name: 'Communion Support', base: 'Support gem', drops: 1, wiki: '10%', notCounted: true },
		{ id: 'astrolabe', name: 'Templar Astrolabe', base: 'Astrolabe', drops: 2, wiki: '—' },
		{ id: 'nooks', name: "Nook's Crown", base: 'Divination card', drops: 0, wiki: '<0.5%', icon: 'divcard' },
		{ id: 'keepers', name: "Keeper's Corruption", base: 'Divination card', drops: 0, wiki: '—', icon: 'divcard' },
		{ id: 'rareveiled', name: 'Rare, two veiled modifiers', base: 'Any rare equipment', drops: 0, wiki: '—', icon: null, manual: true }
	];
	function pct(d, n) { return n ? Math.round((d / n) * 100) + '%' : '—'; }
	function decorate(x) {
		var key = x.icon === undefined ? x.id : x.icon;
		return Object.assign({}, x, {
			src: key ? icons[key] : null,
			rate: x.notCounted ? '—' : pct(x.drops, runs),
			prov: x.notCounted ? 'not counted' : x.drops + '/' + runs
		});
	}
	return {
		icons: icons,
		boss: { name: 'Catarina, Master of Undeath', short: 'Catarina', area: 'BetrayalMastermindFight', level: 83, runs: runs, devices: 1 },
		pool: pool.map(decorate),
		extra: extra.map(decorate),
		// The capture shown on the overlay boards: OCR read (POC run 7 shape) + the
		// magic Spidersilk Robe from the owner's 2026-10-27 screenshot as an unmatched line.
		capture: {
			run: 79, seed: '2667568808',
			matched: ['urn', 'veiledex', 'eye', 'allflame'],
			read: ['SILVER FLASK', 'VEILED EXALTED ORB', "ZORATH'S EYE OF THE ENDLESS", 'ALLFLAME EMBER OF KULEMAK'],
			unknown: ['Spidersilk Robe']
		},
		// Last runs of the seed data (sheet rows 71–78), newest first.
		recent: [
			{ n: 78, items: ['allflame', 'diadem'] }, { n: 77, items: ['diadem'] },
			{ n: 76, items: ['queens'] }, { n: 75, items: ['cane'] },
			{ n: 74, items: ['eye', 'diadem'] }, { n: 73, items: ['diadem'] },
			{ n: 72, items: ['allflame', 'diadem'] }, { n: 71, items: ['eye', 'urn'] }
		],
		// Every run of the owner's sheet, run 1 first (POE-317 round 2: Runs pagination).
		allRuns: [["cane","communion"],["urn"],["urn"],["cane","eye"],["cane","veiledex","allflame"],["urn","eye","veiledex","allflame","astrolabe"],["bitterbind"],["cane","eye"],["urn","eye"],["spinehail","veiledex"],["cane"],["bitterbind","eye","allflame"],["queens","veiledex"],["queens","eye","veiledex"],["cane"],["urn","eye","veiledex","allflame"],["spinehail","eye","allflame"],["urn","eye","veiledex"],["urn","eye"],["spinehail"],["diadem","eye","allflame"],["urn"],["cane","eye","veiledex"],["queens","eye","veiledex","allflame"],["urn","allflame"],["spinehail"],["spinehail","veiledex"],["urn"],["diadem","eye"],["bitterbind","eye"],["spinehail","veiledex"],["spinehail","eye"],["bitterbind"],["urn","eye","veiledex","allflame"],["queens","eye","veiledex"],["urn","eye","veiledex","allflame"],["cane"],["urn","eye"],["urn","eye","veiledex"],["queens","eye"],["queens","veiledex","allflame"],["urn","eye","allflame"],["bitterbind","eye","allflame"],["spinehail","veiledex"],["cane","eye"],["diadem","eye","veiledex","allflame"],["urn"],["urn"],["spinehail","veiledex","allflame"],["spinehail"],["cane","veiledex","allflame"],["spinehail","eye"],["cane"],["urn"],["diadem"],["diadem"],["spinehail","eye","veiledex"],["urn","veiledex","allflame"],["diadem","eye"],["spinehail","eye","veiledex"],["urn","allflame"],["cane"],["spinehail","astrolabe"],["bitterbind","veiledex"],["cane","veiledex"],["urn","veiledex"],["spinehail","allflame"],["urn","allflame"],["spinehail"],["urn","eye"],["urn","eye"],["diadem","allflame"],["diadem"],["diadem","eye"],["cane"],["queens"],["diadem"],["diadem","allflame"]],
		// First five runs of the sheet: the low-n state.
		lowN: { runs: 5, urn: 2, cane: 3, eye: 1, veiledex: 1, allflame: 1 },
		deviceId: '[device id]'
	};
})();
