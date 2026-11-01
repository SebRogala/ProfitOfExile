/* POE-317 round 3: boss profiles in the general shape —
   0..k guaranteed groups (each min/max) + additional drops (named items,
   category tiles, optional quantity). Wiki rates and bases: scout report of
   2026-11-01 (poewiki, 3.24–3.29). Unique names for Maven, Fear and Black Star
   were resolved base → unique on poe.ninja (Allflame) and from training data;
   [placeholders] mark names the scout did not give. Only Catarina has own runs. */
window.POE317P = (function () {
	var D = window.POE317, B = '/_blob/';
	var A = {
		legacy: 'da91a957b4672f0f7a295d549fce0d63', graven: 'db5bb615c9a68bd67c0ea68c895d405f', arn: 'd4dc14347f67b0894b1440886385bfb7',
		olesya: 'fbe4cd7c9a0afaa066cb2e7da0bdf584', doppel: 'a0f29ba0675de386a61fd5b4812be95a', echoforge: '487d63de9684a300c49bbe3e4fff9d3d',
		conflict: '1b4bada120c3508af93cda5df350de3b', intention: '82278658bbc91fc1d5066ba6141185e7', servant: '619d49f4b2656f30b5ce60a49ae47157',
		enmity: '06d0cf46358252772cb1fdb0a2eee023', templar: 'f1f8114668b820e83642cc190406ffba', polaric: '7452e33c552cb774fd2c911a4b125e20',
		dawn: '9ddb3422c83e05a530f2615ca4dc15e7', struggle: '9e80f515abb4947251b4c7c02b2006cb', eannul: '341f399b178b0d281d784d4a537b6c8b',
		echaos: '92592b1d90c63b94369e54470f6f8915', eexalt: 'd7858362e427be224a71c65b087203b3', gember: '34a9ff36ce514b143c76cb28518629a5',
		divcard: 'bf252a6d0ddd38ec200bf48b1d496b12'
	};
	function it(id, name, base, art, wiki, more) { return Object.assign({ id: id, name: name, base: base, src: art ? B + A[art] : null, wiki: wiki }, more || {}); }
	function own(x) { return Object.assign({}, x, { own: x.rate, ownProv: x.prov }); }

	var cat = D.extra.filter(function (x) { return !x.manual; }).map(own);
	return {
		catarina: {
			name: 'Catarina', key: 'Normal', area: D.boss.area, level: D.boss.level, runs: D.boss.runs, run: 79,
			groups: [{ id: 'unique', label: 'Unique', min: 1, max: 1, items: D.pool.map(own) }],
			extra: cat,
			categories: [{ id: 'rareveiled', name: 'Rare, two veiled modifiers', base: 'any rare equipment; OCR cannot see mods', wiki: '—' }],
			capture: { picks: { unique: ['urn'] }, extra: { eye: 1, veiledex: 1, allflame: 1 }, cats: {}, amb: null, unknown: ['Spidersilk Robe'] }
		},
		maven: {
			name: 'The Maven', key: 'Normal pool', area: '[area id]', level: '[NN]', runs: 0, run: 1,
			groups: [{ id: 'unique', label: 'Unique', min: 1, max: 1, items: [
				it('legacy', 'Legacy of Fury', 'Wyrmscale Boots', 'legacy', '—'),
				it('graven', "Graven's Secret", 'Cloth Belt', 'graven', '17%'),
				it('arn', "Arn's Anguish", 'Cloth Belt', 'arn', '17%'),
				it('olesya', "Olesya's Delight", 'Cloth Belt', 'olesya', '17%'),
				it('doppel', 'Doppelgänger Guise', 'Sadist Garb', 'doppel', '—'),
				it('echoforge', 'Echoforge', 'Infernal Sword', 'echoforge', '—')
			] }],
			extra: [
				it('conflict', 'Orb of Conflict', 'Currency', 'conflict', '32%', { qty: true }),
				it('invert', 'Invert the Rules', '[item type]', null, '10%'),
				it('auspicious', 'Auspicious Ambitions', 'Divination card', 'divcard', '<0.05%')
			],
			categories: [],
			capture: { picks: { unique: [] }, extra: { conflict: 2 }, cats: {}, amb: { group: 'unique', base: 'Cloth Belt', ids: ['graven', 'arn', 'olesya'] }, unknown: [] }
		},
		fear: {
			name: 'Incarnation of Fear', key: 'Normal pool', area: '[area id]', level: '[NN]', runs: 0, run: 1,
			groups: [
				{ id: 'unique', label: 'Unique', min: 1, max: 1, items: [
					it('servant', 'Servant of Decay', 'Torturer Garb', 'servant', '—'),
					it('enmity', "Enmity's Embrace", 'Vermillion Ring', 'enmity', '—'),
					it('opalu', '[unique on Opal Ring]', 'Opal Ring', null, '—'),
					it('axeu', '[unique on Abyssal Axe]', 'Abyssal Axe', null, '—')
				] },
				{ id: 'astrolabe', label: 'Astrolabe', min: 1, max: 1, items: [
					it('templar', 'Templar Astrolabe', 'Astrolabe', 'templar', '33%'),
					it('otherastro', '[9 other Astrolabe types]', 'Astrolabe · 7.5% each', null, '—')
				] }
			],
			extra: [
				it('intention', 'Orb of Intention', 'Currency', 'intention', '50%', { qty: true }),
				it('bound', 'Bound by Destiny', '[item type]', null, '10%')
			],
			categories: [{ id: 'memory', name: 'Memory Strand items', base: 'any item with memory strands', wiki: '—' }],
			capture: { picks: { unique: ['servant'], astrolabe: ['templar'] }, extra: { intention: 1 }, cats: { memory: 2 }, amb: null, unknown: [] }
		},
		blackstar: {
			name: 'Black Star', key: 'Polaric Invitation', area: '[area id]', level: '[NN]', runs: 0, run: 1,
			groups: [],
			extra: [
				it('polaric', 'Polaric Devastation', 'Opal Ring', 'polaric', '—'),
				it('dawn', 'Sudden Dawn', 'Steel Circlet', 'dawn', '3–5%'),
				it('struggle', 'The Eternal Struggle', 'Onyx Amulet', 'struggle', '5–10%'),
				it('eannul', 'Eldritch Orb of Annulment', 'Currency', 'eannul', '5–10%', { qty: true }),
				it('echaos', 'Eldritch Chaos Orb', 'Currency', 'echaos', '5–10%', { qty: true }),
				it('eexalt', 'Eldritch Exalted Orb', 'Currency', 'eexalt', '5–10%', { qty: true }),
				it('gember', 'Grand Eldritch Ember', 'Currency', 'gember', '12–20%', { qty: true })
			],
			categories: [],
			capture: { picks: {}, extra: { struggle: 1, gember: 3, echaos: 1 }, cats: {}, amb: null, unknown: [] }
		}
	};
})();
