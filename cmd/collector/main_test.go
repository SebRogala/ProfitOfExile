package main

import (
	"strings"
	"testing"
)

// EXPECTED_LEAGUE is the deploy-time guard that stops a collector from writing
// under a league the operator did not intend. These tests pin the three
// behaviors the guard must have: opt-in absence, pass-on-match, fail-on-mismatch.

func TestCheckExpectedLeague_emptyExpectedIsNoOp(t *testing.T) {
	// Absence of EXPECTED_LEAGUE must not block startup — the runtime_config
	// selection stands on its own.
	if err := checkExpectedLeague("", "Mirage"); err != nil {
		t.Fatalf("empty expected should be a no-op, got error: %v", err)
	}
}

func TestCheckExpectedLeague_matchPasses(t *testing.T) {
	if err := checkExpectedLeague("Mirage", "Mirage"); err != nil {
		t.Fatalf("matching expected and resolved should pass, got error: %v", err)
	}
}

func TestCheckExpectedLeague_mismatchFails(t *testing.T) {
	err := checkExpectedLeague("Standard", "Mirage")
	if err == nil {
		t.Fatal("mismatch between EXPECTED_LEAGUE and resolved league must return an error")
	}
	// The message must name both leagues so an operator can see what was
	// expected versus what the runtime actually resolved.
	if !strings.Contains(err.Error(), "Standard") || !strings.Contains(err.Error(), "Mirage") {
		t.Fatalf("error should name both the expected and resolved league, got: %v", err)
	}
}
