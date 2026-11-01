// Package bossloot stores versioned, device-owned boss observations. Rust owns
// loot profiles and validates effective selections before they cross this boundary.
package bossloot

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
	"time"
	"unicode"
	"unicode/utf8"
)

var (
	ErrInvalid  = errors.New("invalid boss loot request")
	ErrNotFound = errors.New("boss loot run not found")
	ErrConflict = errors.New("boss loot version or merge target conflict")
)

type Profile struct {
	Boss           string `json:"boss"`
	ProfileKey     string `json:"profileKey"`
	ProfileVersion int    `json:"profileVersion"`
}
type Items struct {
	Quantities map[string]int64 `json:"quantities"`
	Counted    []string         `json:"counted"`
}
type Run struct {
	Profile
	ID      string     `json:"id"`
	League  string     `json:"league"`
	Run     int64      `json:"run"`
	Date    *time.Time `json:"date"`
	Seed    *string    `json:"seed"`
	Version int64      `json:"version"`
	Items   Items      `json:"items"`
}
type Target struct {
	ID              string `json:"id"`
	ExpectedVersion int64  `json:"expectedVersion"`
}
type Submit struct {
	Profile
	League               string   `json:"league"` // Informational client snapshot; the submit boundary resolves the authoritative scope.
	Seed                 *string  `json:"seed"`
	Mode                 string   `json:"mode"`
	Items                Items    `json:"items"`
	Target               *Target  `json:"target,omitempty"`
	IncomingPositiveKeys []string `json:"incomingPositiveKeys,omitempty"`
}
type SubmitOutcome struct {
	State               string   `json:"state"`
	Run                 Run      `json:"run"`
	DroppedPositiveKeys []string `json:"droppedPositiveKeys"`
}
type Page struct {
	Total    int64 `json:"total"`
	Page     int   `json:"page"`
	PageSize int   `json:"pageSize"`
	Items    []Run `json:"items"`
}
type ItemStats struct {
	Drops        int64 `json:"drops"`
	EligibleRuns int64 `json:"eligibleRuns"`
}
type Stats struct {
	Runs    int64                `json:"runs"`
	Devices int64                `json:"devices"`
	NextRun int64                `json:"nextRun"`
	Items   map[string]ItemStats `json:"items"`
}

func validKey(key string) bool {
	if len(key) == 0 || len(key) > 512 || !utf8.ValidString(key) || strings.TrimSpace(key) != key {
		return false
	}
	for _, c := range key {
		if unicode.IsControl(c) {
			return false
		}
	}
	return true
}
func (p Profile) Validate() error {
	if !validKey(p.Boss) || !validKey(p.ProfileKey) || p.ProfileVersion <= 0 {
		return fmt.Errorf("%w: boss, profileKey and positive profileVersion required", ErrInvalid)
	}
	return nil
}
func validateKeys(keys []string) error {
	if keys == nil || len(keys) > 512 {
		return fmt.Errorf("%w: key array required (max 512)", ErrInvalid)
	}
	seen := map[string]bool{}
	for _, key := range keys {
		if !validKey(key) || seen[key] {
			return fmt.Errorf("%w: malformed or duplicate item key", ErrInvalid)
		}
		seen[key] = true
	}
	return nil
}
func (i Items) Validate() error {
	if err := validateKeys(i.Counted); err != nil {
		return err
	}
	if i.Quantities == nil || len(i.Quantities) > 512 {
		return fmt.Errorf("%w: quantities object required", ErrInvalid)
	}
	counted := map[string]bool{}
	for _, key := range i.Counted {
		counted[key] = true
	}
	for key, q := range i.Quantities {
		if !validKey(key) || !counted[key] || q <= 0 || q > 9007199254740991 {
			return fmt.Errorf("%w: positive safe integer quantity must belong to counted set", ErrInvalid)
		}
	}
	return nil
}
func ValidateTarget(id string, version int64) error {
	n, err := strconv.ParseInt(id, 10, 64)
	if err != nil || n <= 0 || version <= 0 {
		return fmt.Errorf("%w: positive id and expectedVersion required", ErrInvalid)
	}
	return nil
}
func ValidatePagination(page, size int) error {
	if page < 1 || page > 1000000 || size < 1 || size > 200 {
		return fmt.Errorf("%w: page must be 1..1000000 and pageSize 1..200", ErrInvalid)
	}
	if (page-1)*size > 10000 {
		return fmt.Errorf("%w: offset must not exceed 10000", ErrInvalid)
	}
	return nil
}
func (s Submit) Validate() error {
	if err := s.Profile.Validate(); err != nil {
		return err
	}
	if err := s.Items.Validate(); err != nil {
		return err
	}
	if s.Seed != nil && !validKey(*s.Seed) {
		return fmt.Errorf("%w: malformed seed", ErrInvalid)
	}
	switch s.Mode {
	case "new", "forceNew":
		if s.Target != nil || s.IncomingPositiveKeys != nil {
			return fmt.Errorf("%w: merge metadata on new run", ErrInvalid)
		}
	case "merge":
		if s.Target == nil || s.Seed == nil {
			return fmt.Errorf("%w: merge target and seed required", ErrInvalid)
		}
		if err := ValidateTarget(s.Target.ID, s.Target.ExpectedVersion); err != nil {
			return err
		}
		if err := validateKeys(s.IncomingPositiveKeys); err != nil {
			return err
		}
	default:
		return fmt.Errorf("%w: mode must be new, forceNew or merge", ErrInvalid)
	}
	return nil
}
