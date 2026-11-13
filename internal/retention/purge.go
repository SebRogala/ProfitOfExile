// Package retention enforces the device-data retention policy: rows older than
// the retention period lose their device link, inactive devices are deleted and
// inactive banned devices keep only their ban record.
package retention

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
)

// RetentionYears is the retention period: rows older than this lose their
// device link, and devices unseen for this long are purged. Calendar years, so
// the cutoff is the same date two years back across leap days.
const RetentionYears = 2

// Counts is the number of rows one Purge changed, per table.
type Counts struct {
	FontSessions   int64 // device_id/pair_code nulled
	LabRuns        int64 // device_id nulled
	BossLootRuns   int64 // device_id nulled by age (rows of deleted devices are nulled by the FK and not counted)
	MercTemplates  int64 // device_id nulled
	DevicesDeleted int64 // inactive, not banned
	DevicesCleared int64 // inactive banned: alias/app_version cleared, role reset
}

// Purger applies the retention policy in one transaction per run.
type Purger struct {
	pool *pgxpool.Pool
	// now is the clock the cutoff is computed from, injectable for tests.
	now func() time.Time
}

// NewPurger creates a Purger on the wall clock.
func NewPurger(pool *pgxpool.Pool) *Purger {
	return &Purger{pool: pool, now: time.Now}
}

// Cutoff is the oldest instant still inside the retention period.
func (p *Purger) Cutoff() time.Time {
	return p.now().AddDate(-RetentionYears, 0, 0)
}

const (
	anonymiseFontSessionsSQL = `UPDATE font_sessions SET device_id = NULL, pair_code = NULL
		WHERE time < $1 AND (device_id IS NOT NULL OR pair_code IS NOT NULL)`
	anonymiseLabRunsSQL = `UPDATE lab_runs SET device_id = NULL
		WHERE created_at < $1 AND device_id IS NOT NULL`
	anonymiseBossLootRunsSQL = `UPDATE boss_loot_runs SET device_id = NULL
		WHERE created_at < $1 AND device_id IS NOT NULL`
	anonymiseMercTemplatesSQL = `UPDATE merc_icon_templates SET device_id = NULL
		WHERE created_at < $1 AND device_id IS NOT NULL`

	// The row lock makes a concurrent device.Repository.Upsert of a candidate
	// wait until this transaction commits, after which it inserts a fresh row.
	lockInactiveDevicesSQL = `SELECT fingerprint FROM devices
		WHERE last_seen < $1 AND NOT banned FOR UPDATE`
	unlinkFontSessionsSQL  = `UPDATE font_sessions SET device_id = NULL, pair_code = NULL WHERE device_id = ANY($1)`
	unlinkLabRunsSQL       = `UPDATE lab_runs SET device_id = NULL WHERE device_id = ANY($1)`
	unlinkMercTemplatesSQL = `UPDATE merc_icon_templates SET device_id = NULL WHERE device_id = ANY($1)`
	// boss_loot_runs follows via its ON DELETE SET NULL foreign key.
	deleteDevicesSQL = `DELETE FROM devices WHERE fingerprint = ANY($1)`

	clearBannedDevicesSQL = `UPDATE devices SET alias = NULL, app_version = NULL, role = 'user'
		WHERE banned AND last_seen < $1
		  AND (alias IS NOT NULL OR app_version IS NOT NULL OR role <> 'user')`
)

// Purge applies the policy once, all statements in one transaction.
func (p *Purger) Purge(ctx context.Context) (Counts, error) {
	cutoff := p.Cutoff()
	var c Counts
	err := pgx.BeginFunc(ctx, p.pool, func(tx pgx.Tx) error {
		exec := func(dst *int64, sql string, arg any) error {
			tag, err := tx.Exec(ctx, sql, arg)
			if err != nil {
				return err
			}
			*dst += tag.RowsAffected()
			return nil
		}
		for _, s := range []struct {
			dst *int64
			sql string
		}{
			{&c.FontSessions, anonymiseFontSessionsSQL},
			{&c.LabRuns, anonymiseLabRunsSQL},
			{&c.BossLootRuns, anonymiseBossLootRunsSQL},
			{&c.MercTemplates, anonymiseMercTemplatesSQL},
		} {
			if err := exec(s.dst, s.sql, cutoff); err != nil {
				return fmt.Errorf("anonymise: %w", err)
			}
		}

		rows, err := tx.Query(ctx, lockInactiveDevicesSQL, cutoff)
		if err != nil {
			return fmt.Errorf("lock inactive devices: %w", err)
		}
		inactive, err := pgx.CollectRows(rows, pgx.RowTo[string])
		if err != nil {
			return fmt.Errorf("lock inactive devices: %w", err)
		}
		if len(inactive) > 0 {
			for _, s := range []struct {
				dst *int64
				sql string
			}{
				{&c.FontSessions, unlinkFontSessionsSQL},
				{&c.LabRuns, unlinkLabRunsSQL},
				{&c.MercTemplates, unlinkMercTemplatesSQL},
				{&c.DevicesDeleted, deleteDevicesSQL},
			} {
				if err := exec(s.dst, s.sql, inactive); err != nil {
					return fmt.Errorf("purge inactive devices: %w", err)
				}
			}
		}

		if err := exec(&c.DevicesCleared, clearBannedDevicesSQL, cutoff); err != nil {
			return fmt.Errorf("clear banned devices: %w", err)
		}
		return nil
	})
	if err != nil {
		return Counts{}, fmt.Errorf("retention purge: %w", err)
	}
	return c, nil
}

// Run purges once at start and then every interval until ctx ends. A failed
// run is logged and retried on the next tick.
func (p *Purger) Run(ctx context.Context, interval time.Duration) {
	ticker := time.NewTicker(interval)
	defer ticker.Stop()
	for {
		p.runOnce(ctx)
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}

func (p *Purger) runOnce(ctx context.Context) {
	c, err := p.Purge(ctx)
	if err != nil {
		if ctx.Err() == nil {
			slog.Error("retention purge failed; retrying next tick", "error", err)
		}
		return
	}
	slog.Info("retention purge done",
		"cutoff", p.Cutoff(),
		"fontSessions", c.FontSessions,
		"labRuns", c.LabRuns,
		"bossLootRuns", c.BossLootRuns,
		"mercTemplates", c.MercTemplates,
		"devicesDeleted", c.DevicesDeleted,
		"devicesCleared", c.DevicesCleared,
	)
}
