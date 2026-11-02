/* POE-328 / POE-329 shared sample dataset — every board reads it.
   Exceptional supports: gem_snapshots (poe.ninja), league Mirage, 2026-09-09 00:33 UTC,
   variant "1", uncorrupted — real values.
   Transfigure option text: docs/GAME-FACTS.md (font option table).
   Facetor's Lens and the support feeder: ILLUSTRATIVE (no trade-queue price exists yet). */
window.FONT_FIXTURE = {
	exceptional: {
		option: 'Exchange a Support Gem for a random Empower, Enlighten, or Enhance Support',
		source: 'poe.ninja',
		age: '6m ago',
		staleAge: '3h ago',
		gems: [
			{ name: 'Enhance Support', price: '9c', listed: 804 },
			{ name: 'Enlighten Support', price: '185c', listed: 735 },
			{ name: 'Empower Support', price: '43c', listed: 1098 }
		]
	},
	transfigure: {
		option: 'Transform a non-Transfigured Skill Gem to a Transfigured version',
		note: 'You choose the version'
	},
	lens: {
		option: 'Sacrifice a Gem to gain 60% of its experience as a Facetor’s Lens',
		lensName: 'Facetor’s Lens',
		lensSub: 'stored experience ≥ 999,627,000',
		supportName: 'Exceptional Support Gem lv 3+',
		source: 'trade · median of the 3 lowest',
		age: '18m ago',
		staleAge: '3h ago',
		priced: { lens: 412, support: 95 },
		negative: { lens: 60, support: 95 }
	}
};
