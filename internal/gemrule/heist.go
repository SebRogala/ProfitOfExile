// Package gemrule holds gem rules that more than one package applies and that
// no single domain package can own: the collector's storage drop, lab
// eligibility, and the backtest/run-v2 queries all read the Heist-only rule
// from here.
//
// The Go predicate and the SQL fragment must stay equivalent;
// TestSQLExclusionsMatchGoPredicates (internal/lab, integration tag) pins the
// pairing against a real database.
package gemrule

import "strings"

// SQLNotHeistOnly is the SQL form of IsHeistOnlyName, for queries over a
// `name` column that must apply the rule in the database.
const SQLNotHeistOnly = `name NOT LIKE '%Trarthus%'`

// IsHeistOnlyName reports whether a gem is Heist-exclusive and therefore
// unobtainable in standard league play.
//
// Game rule: the "of Trarthus" transfigured gems drop only from Heist
// blueprints, so the lab can neither hand one out nor be fed one. The collector
// refuses to store them at all (internal/collector/ninja.go), and measured
// 2026-08-05 the local gem_snapshots holds 0 Trarthus rows — but gem_colors
// still carries 12 seeded Trarthus names, so any surface reading gem_colors
// rather than snapshots has to apply this rule itself.
func IsHeistOnlyName(name string) bool {
	return strings.Contains(name, "Trarthus")
}
