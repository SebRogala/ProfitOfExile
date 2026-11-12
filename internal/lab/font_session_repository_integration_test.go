//go:build integration

package lab

import (
	"context"
	"errors"
	"fmt"
	"testing"
	"time"
)

func TestFontSessionRepository_FailedRoundLeavesNoSession(t *testing.T) {
	pool := labIntegrationPool(t)
	dev := fmt.Sprintf("poe344-font-repo-%s-%d", t.Name(), time.Now().UnixNano())
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM font_sessions WHERE device_id = $1`, dev); err != nil {
			t.Error(err)
		}
	})

	_, err := NewFontSessionRepository(pool).Store(context.Background(), dev, FontSession{
		LabType: "Uber", TotalCrafts: 6, Variant: "20/20",
		Rounds: []FontRound{{OptionChosen: "ok"}, {OptionChosen: "bad\x00"}},
	})

	var storeErr *FontSessionStoreError
	if !errors.As(err, &storeErr) || storeErr.Stage != FontSessionStageInsertRound || storeErr.RoundIndex != 1 {
		t.Fatalf("err = %#v, want *FontSessionStoreError at insert round, index 1", err)
	}
	var n int
	if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM font_sessions WHERE device_id = $1`, dev).Scan(&n); err != nil || n != 0 {
		t.Fatalf("font_sessions rows = %d (err %v), want 0 after rollback", n, err)
	}
}
