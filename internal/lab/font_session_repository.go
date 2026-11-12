package lab

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/jackc/pgx/v5/pgxpool"
)

// FontSession is one Divine Font encounter reported by the desktop OCR
// pipeline. Variant is stored as given; callers apply any default.
type FontSession struct {
	LabType     string
	TotalCrafts int
	Variant     string
	PairCode    string
	Rounds      []FontRound
}

// FontRound is one craft round within a font session. Empty OptionChosen,
// GemPicked and GemsOffered are stored as NULL.
type FontRound struct {
	CraftOptions    []FontCraftOption
	OptionChosen    string
	GemsOffered     []string
	GemPicked       string
	CraftsRemaining *int
}

// FontCraftOption is one offered craft. Its JSON tags define the stored
// font_rounds.craft_options JSONB.
type FontCraftOption struct {
	Type  string `json:"type"`
	Text  string `json:"text"`
	Value *int   `json:"value,omitempty"`
}

// FontSessionStage names the step of the store transaction that failed.
type FontSessionStage string

const (
	FontSessionStageBegin          FontSessionStage = "begin"
	FontSessionStageInsertSession  FontSessionStage = "insert session"
	FontSessionStageMarshalOptions FontSessionStage = "marshal options"
	FontSessionStageInsertRound    FontSessionStage = "insert round"
	FontSessionStageCommit         FontSessionStage = "commit"
)

// FontSessionStoreError reports which stage of Store failed. RoundIndex is set
// for the marshal-options and insert-round stages; SessionID for insert-round.
type FontSessionStoreError struct {
	Stage      FontSessionStage
	RoundIndex int
	SessionID  int64
	Err        error
}

func (e *FontSessionStoreError) Error() string {
	return fmt.Sprintf("font session repo: %s: %v", e.Stage, e.Err)
}

func (e *FontSessionStoreError) Unwrap() error { return e.Err }

// FontSessionRepository persists font sessions. font_sessions is keyed by
// device, not league.
type FontSessionRepository struct {
	pool *pgxpool.Pool
}

// NewFontSessionRepository creates a FontSessionRepository backed by the given pool.
func NewFontSessionRepository(pool *pgxpool.Pool) *FontSessionRepository {
	return &FontSessionRepository{pool: pool}
}

// Store inserts the session and its rounds (numbered from 1) in one
// transaction and returns the session id. Errors are *FontSessionStoreError.
func (r *FontSessionRepository) Store(ctx context.Context, deviceID string, s FontSession) (int64, error) {
	tx, err := r.pool.Begin(ctx)
	if err != nil {
		return 0, &FontSessionStoreError{Stage: FontSessionStageBegin, Err: err}
	}
	defer tx.Rollback(ctx)

	var sessionID int64
	err = tx.QueryRow(ctx,
		`INSERT INTO font_sessions (lab_type, total_crafts, variant, device_id, pair_code)
		 VALUES ($1, $2, $3, $4, $5)
		 RETURNING id`,
		s.LabType,
		s.TotalCrafts,
		s.Variant,
		deviceID,
		s.PairCode,
	).Scan(&sessionID)
	if err != nil {
		return 0, &FontSessionStoreError{Stage: FontSessionStageInsertSession, Err: err}
	}

	for i, round := range s.Rounds {
		optionsJSON, err := json.Marshal(round.CraftOptions)
		if err != nil {
			return 0, &FontSessionStoreError{Stage: FontSessionStageMarshalOptions, RoundIndex: i, SessionID: sessionID, Err: err}
		}

		if _, err := tx.Exec(ctx,
			`INSERT INTO font_rounds (session_id, round_number, craft_options, option_chosen, gems_offered, gem_picked, crafts_remaining)
			 VALUES ($1, $2, $3, $4, $5, $6, $7)`,
			sessionID,
			i+1,
			optionsJSON,
			nilIfEmpty(round.OptionChosen),
			pgStringArray(round.GemsOffered),
			nilIfEmpty(round.GemPicked),
			round.CraftsRemaining,
		); err != nil {
			return 0, &FontSessionStoreError{Stage: FontSessionStageInsertRound, RoundIndex: i, SessionID: sessionID, Err: err}
		}
	}

	if err := tx.Commit(ctx); err != nil {
		return 0, &FontSessionStoreError{Stage: FontSessionStageCommit, Err: err}
	}
	return sessionID, nil
}

func nilIfEmpty(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}

func pgStringArray(ss []string) any {
	if len(ss) == 0 {
		return nil
	}
	return ss
}
