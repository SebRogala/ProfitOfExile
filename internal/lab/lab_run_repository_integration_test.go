//go:build integration

package lab

import (
	"context"
	"errors"
	"fmt"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// labRunDevice returns a device id unique to this test and deletes its runs
// (rooms cascade) when the test ends.
func labRunDevice(t *testing.T, pool *pgxpool.Pool) string {
	t.Helper()
	dev := fmt.Sprintf("poe344-repo-%s-%d", t.Name(), time.Now().UnixNano())
	t.Cleanup(func() {
		if _, err := pool.Exec(context.Background(), `DELETE FROM lab_runs WHERE device_id = $1`, dev); err != nil {
			t.Error(err)
		}
	})
	return dev
}

func sampleLabRun(rooms ...LabRunRoom) LabRun {
	return LabRun{
		Difficulty: "Uber", Strategy: "speed", StartedAt: time.Date(2026, 11, 12, 10, 0, 0, 0, time.UTC),
		ElapsedSeconds: 300, RoomCount: 8, Rooms: rooms,
	}
}

func TestLabRunRepository_FailedRoomInsertLeavesNoRun(t *testing.T) {
	pool := labIntegrationPool(t)
	dev := labRunDevice(t, pool)
	repo := NewLabRunRepository(pool)
	at := time.Date(2026, 11, 12, 10, 0, 0, 0, time.UTC)

	_, err := repo.Store(context.Background(), dev, sampleLabRun(
		LabRunRoom{RoomNumber: 1, RoomName: "ok", EnteredAt: at},
		LabRunRoom{RoomNumber: 2, RoomName: "bad\x00name", EnteredAt: at},
	))

	var storeErr *LabRunStoreError
	if !errors.As(err, &storeErr) || storeErr.Stage != LabRunStageInsertRoom || storeErr.RoomIndex != 1 {
		t.Fatalf("err = %#v, want *LabRunStoreError at insert room, index 1", err)
	}
	var n int
	if err := pool.QueryRow(context.Background(), `SELECT count(*) FROM lab_runs WHERE device_id = $1`, dev).Scan(&n); err != nil || n != 0 {
		t.Fatalf("lab_runs rows = %d (err %v), want 0 after rollback", n, err)
	}
}

func TestLabRunRepository_StoredRoomsReadBackUnderTheirRun(t *testing.T) {
	pool := labIntegrationPool(t)
	dev := labRunDevice(t, pool)
	repo := NewLabRunRepository(pool)
	want := []LabRunRoom{
		{RoomNumber: 1, RoomName: "Estate Path", EnteredAt: time.Date(2026, 11, 12, 10, 0, 0, 0, time.UTC)},
		{RoomNumber: 2, RoomName: "Basilica Halls", EnteredAt: time.Date(2026, 11, 12, 10, 1, 30, 0, time.UTC)},
	}

	runID, err := repo.Store(context.Background(), dev, sampleLabRun(want...))
	if err != nil {
		t.Fatal(err)
	}

	rows, err := pool.Query(context.Background(),
		`SELECT room_number, room_name, entered_at FROM lab_run_rooms WHERE run_id = $1 ORDER BY room_number`, runID)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	var got []LabRunRoom
	for rows.Next() {
		var room LabRunRoom
		if err := rows.Scan(&room.RoomNumber, &room.RoomName, &room.EnteredAt); err != nil {
			t.Fatal(err)
		}
		got = append(got, room)
	}
	if err := rows.Err(); err != nil {
		t.Fatal(err)
	}
	if len(got) != len(want) {
		t.Fatalf("rooms = %+v, want %+v", got, want)
	}
	for i := range want {
		if got[i].RoomNumber != want[i].RoomNumber || got[i].RoomName != want[i].RoomName || !got[i].EnteredAt.Equal(want[i].EnteredAt) {
			t.Fatalf("room %d = %+v, want %+v", i, got[i], want[i])
		}
	}
}

func TestLabRunRepository_StoreErrorNamesTheStage(t *testing.T) {
	pool := labIntegrationPool(t)
	dev := labRunDevice(t, pool)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	_, err := NewLabRunRepository(pool).Store(ctx, dev, sampleLabRun())

	var storeErr *LabRunStoreError
	if !errors.As(err, &storeErr) || storeErr.Stage != LabRunStageBegin {
		t.Fatalf("err = %#v, want *LabRunStoreError at begin", err)
	}
}
