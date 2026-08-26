//go:build integration

package mercenary

import (
	"context"
	"errors"
	"testing"

	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// The signature column's length CHECK is the only thing standing between a row
// whose bytes cannot be decoded and every device that pulls its format version.
// POE-207 (20260827003001_merc_signature_v2_length_check) made it
// version-CONDITIONAL, and "conditional" is exactly what these four tests
// defend: the tempting simplification — `octet_length(signature) IN (576,
// 1728)` — accepts all four cases below, including the two that must fail.
//
// The insert is raw SQL rather than the repository, on purpose. NewSignature
// refuses a 576-byte payload outright, so going through Accept can only ever
// exercise the length the Go type already agrees with; the only way to ask the
// DATABASE what it permits is around the type.

// checkFamily is a sentinel name, not a reserveFamily borrowing from the
// vocabulary: these rows never become a Key, they are only ever counted by the
// database, and a name no vocabulary holds cannot collide with a test that does
// take a real family.
const checkFamily = "__signature_check__"

// clearCheckFamily removes the sentinel rows before and after a test. The
// post-test half is not tidiness: phase 1 of the integration script runs the
// migration package's down migrations, and the down half of this migration
// FAILS while a format-2 row exists (which is its documented, intended
// behaviour). A leaked row here reappears as an unrelated package going red.
func clearCheckFamily(t *testing.T, pool *pgxpool.Pool) {
	t.Helper()
	clear := func(when string) {
		if _, err := pool.Exec(context.Background(),
			`DELETE FROM merc_icon_templates WHERE family = $1`, checkFamily); err != nil {
			t.Fatalf("%s cleanup for %q: %v", when, checkFamily, err)
		}
	}
	clear("pre-test")
	t.Cleanup(func() { clear("post-test") })
}

// insertRawLength writes one row of the given format version and signature
// length straight to the table, and returns the database's answer. Shared with
// repository_integration_test.go, which uses it to plant the format-1 rows the
// migration deliberately leaves behind.
func insertRawLength(t *testing.T, pool *pgxpool.Pool, family string, formatVersion int16, sigLen int) error {
	t.Helper()
	_, err := pool.Exec(context.Background(),
		`INSERT INTO merc_icon_templates (family, tier, format_version, signature, device_id)
		 VALUES ($1, 1, $2, $3, 'signature-check')`,
		family, formatVersion, make([]byte, sigLen))
	return err
}

// isSignatureCheckViolation reports whether the error is the signature CHECK
// refusing the row, rather than any other failure. Naming the constraint
// matters: a NOT NULL or a tier violation would otherwise read as a pass.
func isSignatureCheckViolation(err error) bool {
	var pgErr *pgconn.PgError
	return errors.As(err, &pgErr) &&
		pgErr.Code == "23514" &&
		pgErr.ConstraintName == "merc_icon_templates_signature_check"
}

// Format-1 rows are neither migrated nor deleted — they are still exactly right
// for a device that asks for version 1, and they must stay insertable while
// such devices exist.
func TestSignatureLengthCheck_AcceptsA576ByteFormat1Row(t *testing.T) {
	pool := integrationPool(t)
	clearCheckFamily(t, pool)

	if err := insertRawLength(t, pool, checkFamily, 1, 576); err != nil {
		t.Fatalf("insert 576-byte format-1 row: %v", err)
	}
}

// The point of the migration: 1728 bytes is what a format-2 signature is, and
// before it the column refused every single one.
func TestSignatureLengthCheck_AcceptsA1728ByteFormat2Row(t *testing.T) {
	pool := integrationPool(t)
	clearCheckFamily(t, pool)

	if err := insertRawLength(t, pool, checkFamily, SupportedFormatVersion, SigBytes); err != nil {
		t.Fatalf("insert %d-byte format-%d row: %v", SigBytes, SupportedFormatVersion, err)
	}
}

// A format-2-sized signature stamped format 1 is the row a flat length list
// would let in — and it is then served to every device still asking for version
// 1, each of which refuses it or reads it as garbage, silently and one at a
// time. The version and the length have to agree.
func TestSignatureLengthCheck_RejectsA1728ByteRowStampedFormat1(t *testing.T) {
	pool := integrationPool(t)
	clearCheckFamily(t, pool)

	err := insertRawLength(t, pool, checkFamily, 1, SigBytes)
	if err == nil {
		t.Fatal("a 1728-byte signature was accepted under format_version 1")
	}
	if !isSignatureCheckViolation(err) {
		t.Fatalf("insert failed with %v, want the merc_icon_templates_signature_check violation", err)
	}
}

// The mirror image, and the other half of what "conditional" buys: a leftover
// format-1 signature re-stamped as format 2 would correlate against nothing and
// still occupy one of the three slots under a key no hover could then fill.
func TestSignatureLengthCheck_RejectsA576ByteRowStampedFormat2(t *testing.T) {
	pool := integrationPool(t)
	clearCheckFamily(t, pool)

	err := insertRawLength(t, pool, checkFamily, SupportedFormatVersion, 576)
	if err == nil {
		t.Fatal("a 576-byte signature was accepted under format_version 2")
	}
	if !isSignatureCheckViolation(err) {
		t.Fatalf("insert failed with %v, want the merc_icon_templates_signature_check violation", err)
	}
}

// A version the CHECK does not name satisfies neither branch. That is the
// closed-list property the migration header calls a deploy-order constraint:
// the branch for a future format has to ship BEFORE the server that writes it,
// and this test is what makes the closure observable rather than a claim in a
// comment. It also pins the failure to the CONSTRAINT — `format_version > 0`
// would happily take a 3, so without naming the signature check here a passing
// test could be reporting the wrong refusal.
func TestSignatureLengthCheck_RejectsAnUnknownFormatVersion(t *testing.T) {
	pool := integrationPool(t)
	clearCheckFamily(t, pool)

	// Both KNOWN lengths, under a version that names neither. Trying only one
	// would leave open the reading that some length is universally acceptable.
	for _, sigLen := range []int{576, SigBytes} {
		err := insertRawLength(t, pool, checkFamily, 3, sigLen)
		if err == nil {
			t.Fatalf("a %d-byte signature was accepted under format_version 3", sigLen)
		}
		if !isSignatureCheckViolation(err) {
			t.Fatalf("insert of %d bytes at format 3 failed with %v, want the "+
				"merc_icon_templates_signature_check violation", sigLen, err)
		}
	}
}
