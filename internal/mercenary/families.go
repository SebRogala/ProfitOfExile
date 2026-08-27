package mercenary

// Code generated from desktop/src/lib/mercenaries/__fixtures__/mercenary-stats.json.
// DO NOT EDIT by hand — regenerate when the fixture changes.
//
// The mercenary support vocabulary's icon FAMILIES, derived exactly the way
// desktop/src-tauri/src/mercenary/vocab.rs derives them (vocab.rs:131-173):
// take every entry whose id starts with `mercenary.support_`, strip a trailing
// ` (Tier N)` suffix, strip a leading grade word (`Lesser `, `Greater `,
// `Gilded `), then fold the result through the family alias table
// (`vocab.rs:51-68`). `Lesser Chain (Tier 1)`, `Chain (Tier 2)` and
// `Gilded Chain (Tier 3)` are three tiers of the one family `Chain`; the alias
// table is the only place two different display names become one family, and
// it is a closed list of whole names rather than a prefix rule (POE-211).
//
// Why the server carries this at all: `family` arrives as free text from a
// spoofable client, and without a closed set one device can create unbounded
// keys — 459 real keys (153 families x 3 tiers), or as many rows as the rate
// limit allows, all of them art nothing will ever match. Validating against
// the shipped vocabulary makes
// the key space finite and equal to the vocabulary the desktop resolves
// against. It is a fixed list because the vocabulary ships with a build; a new
// league's supports arrive as a fixture change on both sides, and
// families_test.go re-derives this map from the fixture so the two cannot
// drift silently.
//
// Skill entries are deliberately excluded: only supports are read from a cell
// icon, so only supports can key a template.
//
// DEPLOY ORDER: the server must ship no later than a change to the fixture.
// This map and the desktop's vocabulary come from the same file, but they ship
// on separate pipelines (deploy.yml on merge to main; desktop.yml on a
// v-desktop-* tag), so a desktop released first knows families this server does
// not and every upload of one is refused. That is a degradation, not an outage
// — the pool simply does not learn the new families — and it is reported rather
// than silent: the upload response carries `rejected_unknown_family` and the
// served corpus carries `known_family_count`.
//
// The ORDER is unchanged when the change SHRINKS the set — an alias that folds
// two families into one, or a support GGG removed: server first either way.
// What inverts is WHICH SIDE GETS REFUSED. On a growth the desktop is the one
// that can run ahead, and its uploads of the new name are refused. On a shrink
// the server is ahead by construction, and it refuses uploads of the DROPPED
// name from desktops still deriving it — that refusal is the intended outcome,
// because those uploads would key art nothing will ever match again.
var knownFamilies = map[string]struct{}{
	"Added Chaos":                     {},
	"Added Cold":                      {},
	"Added Fire":                      {},
	"Added Lightning":                 {},
	"Additional Duration":             {},
	"Additional Fissures":             {},
	"Additional Leech":                {},
	"Additional Pods":                 {},
	"Additional Pulses":               {},
	"Ailment Damage":                  {},
	"Ailment Effect":                  {},
	"Arcane Traps":                    {},
	"Archers":                         {},
	"Area of Effect":                  {},
	"Area per Projectile":             {},
	"Arrow Nova":                      {},
	"Ash":                             {},
	"Astral Totem":                    {},
	"Beam Width":                      {},
	"Bitterwound":                     {},
	"Blasting":                        {},
	"Brittle Chance":                  {},
	"Brutality":                       {},
	"Cascade Count":                   {},
	"Caustic Conversion":              {},
	"Chain":                           {},
	"Chain Distance":                  {},
	"Chance to Bleed":                 {},
	"Chance to Poison":                {},
	"Chaos Penetration":               {},
	"Charged Traps":                   {},
	"Clone Speed":                     {},
	"Cold Penetration":                {},
	"Combustion":                      {},
	"Concentrated Effect":             {},
	"Conflagrant":                     {},
	"Consecration":                    {},
	"Cooldown Recovery":               {},
	"Critical Chance":                 {},
	"Critical Damage":                 {},
	"Crush":                           {},
	"Cull":                            {},
	"Curse Effect":                    {},
	"Damage from Life":                {},
	"Devotion":                        {},
	"Divine Shield":                   {},
	"DoT Multiplier":                  {},
	"Electrocuting":                   {},
	"Elemental Damage with Attacks":   {},
	"Elemental Focus":                 {},
	"Elemental Weakness on Hit":       {},
	"Empowered Link":                  {},
	"Ensnare Effect":                  {},
	"Excommunicate":                   {},
	"Exposure on Hit":                 {},
	"Extra Armaments":                 {},
	"Extra Targets":                   {},
	"Faster Attacks":                  {},
	"Faster Casting":                  {},
	"Faster Cyclone":                  {},
	"Faster Projectiles":              {},
	"Fire Penetration":                {},
	"Fist of War":                     {},
	"Fork":                            {},
	"Fortification":                   {},
	"Fortify":                         {},
	"Freeze Chance":                   {},
	"Freezer Burn":                    {},
	"Frenzy":                          {},
	"Generosity":                      {},
	"Grasp":                           {},
	"Hallow":                          {},
	"Heft":                            {},
	"Hinder Duration":                 {},
	"Hopelessness":                    {},
	"Hypothermia":                     {},
	"Icecrash Radius":                 {},
	"Ignite Chance":                   {},
	"Impale Chance":                   {},
	"Impale Extraction":               {},
	"Increased Angle":                 {},
	"Infused Channelling":             {},
	"Inhibitor":                       {},
	"Inversion":                       {},
	"Ironwood":                        {},
	"Jolt":                            {},
	"Knockback":                       {},
	"Leech":                           {},
	"Less Duration":                   {},
	"Lightning Penetration":           {},
	"Lucky Lightning Damage":          {},
	"Lumbering Dead":                  {},
	"Maim":                            {},
	"Malediction":                     {},
	"Maximum Explosions":              {},
	"Maximum Shock Effect":            {},
	"Maximum Spikes":                  {},
	"Maximum Stages":                  {},
	"Maximum Storms":                  {},
	"Melee Physical Damage":           {},
	"Melee Splash":                    {},
	"Minion Caustic Death":            {},
	"Minion Damage":                   {},
	"Minion Life":                     {},
	"Mirage Archer":                   {},
	"Mitigation Ignore":               {},
	"Molten Eruption":                 {},
	"More Duration":                   {},
	"Multiple Projectiles":            {},
	"Multiple Totems":                 {},
	"Multiple Traps":                  {},
	"Multistrike":                     {},
	"Nova":                            {},
	"Onslaught on Cry":                {},
	"Physical Damage Reduction":       {},
	"Physical as Extra":               {},
	"Physical as Extra Chaos":         {},
	"Pierce":                          {},
	"Power Charge on Critical Strike": {},
	"Pulverise":                       {},
	"Purified Ground":                 {},
	"Rage on Hit":                     {},
	"Raging Cry":                      {},
	"Reactor":                         {},
	"Relic Recovery":                  {},
	"Return":                          {},
	"Sacred Wisps":                    {},
	"Scattershot":                     {},
	"Scorch Chance":                   {},
	"Searing Agony":                   {},
	"Second Wind":                     {},
	"Secondary Shots":                 {},
	"Shield Damage":                   {},
	"Shock Chance":                    {},
	"Slower Projectiles":              {},
	"Snaking":                         {},
	"Spell Cascade":                   {},
	"Sphere Frequency":                {},
	"Strike Distance":                 {},
	"Stun Duration":                   {},
	"Swift Affliction":                {},
	"Throwing Speed":                  {},
	"Totemic Onslaught":               {},
	"Trap and Mine Damage":            {},
	"Trauma Duration":                 {},
	"Trigger Radius":                  {},
	"Uncertainty":                     {},
	"Volleys":                         {},
	"Voltage":                         {},
	"Voltaxic Conversion":             {},
	"Warcry Speed":                    {},
	"Wither Stacks":                   {},
	"Wither on Hit":                   {},
}
