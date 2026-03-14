package handlers

import (
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"strconv"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// SnapshotQuery holds parsed query parameters for snapshot endpoints.
type SnapshotQuery struct {
	From   time.Time
	To     time.Time
	Name   string
	Limit  int
	Offset int
}

func parseSnapshotQuery(r *http.Request) (*SnapshotQuery, error) {
	q := &SnapshotQuery{
		From:  time.Now().UTC().Add(-24 * time.Hour),
		To:    time.Now().UTC(),
		Limit: 1000,
	}

	if v := r.URL.Query().Get("from"); v != "" {
		t, err := time.Parse(time.RFC3339, v)
		if err != nil {
			return nil, fmt.Errorf("invalid 'from': %w", err)
		}
		q.From = t
	}
	if v := r.URL.Query().Get("to"); v != "" {
		t, err := time.Parse(time.RFC3339, v)
		if err != nil {
			return nil, fmt.Errorf("invalid 'to': %w", err)
		}
		q.To = t
	}
	if v := r.URL.Query().Get("name"); v != "" {
		q.Name = v
	}
	if v := r.URL.Query().Get("limit"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil {
			return nil, fmt.Errorf("limit must be a positive integer, got %q", v)
		}
		if n < 1 {
			return nil, fmt.Errorf("limit must be at least 1, got %d", n)
		}
		if n > 10000 {
			n = 10000
		}
		q.Limit = n
	}
	if v := r.URL.Query().Get("offset"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil {
			return nil, fmt.Errorf("offset must be a non-negative integer, got %q", v)
		}
		if n < 0 {
			return nil, fmt.Errorf("offset must be non-negative, got %d", n)
		}
		q.Offset = n
	}

	return q, nil
}

func writeQueryError(w http.ResponseWriter, msg string, err error) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusBadRequest)
	json.NewEncoder(w).Encode(map[string]string{"error": msg + ": " + err.Error()})
}

// GemSnapshots returns gem snapshot data filtered by time range, name, with pagination.
func GemSnapshots(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		q, err := parseSnapshotQuery(r)
		if err != nil {
			writeQueryError(w, "invalid query parameters", err)
			return
		}

		query := `SELECT time, name, variant, COALESCE(chaos, 0), COALESCE(listings, 0),
		                 is_transfigured, is_corrupted, COALESCE(gem_color, '')
		          FROM gem_snapshots
		          WHERE time >= $1 AND time <= $2`
		args := []any{q.From, q.To}

		if q.Name != "" {
			query += ` AND name = $3`
			args = append(args, q.Name)
			query += ` ORDER BY time DESC, variant LIMIT $4 OFFSET $5`
			args = append(args, q.Limit, q.Offset)
		} else {
			query += ` ORDER BY time DESC, name, variant LIMIT $3 OFFSET $4`
			args = append(args, q.Limit, q.Offset)
		}

		rows, err := pool.Query(r.Context(), query, args...)
		if err != nil {
			slog.Error("gem snapshots: query failed", "error", err, "from", q.From, "to", q.To, "name", q.Name)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		defer rows.Close()

		type gemRow struct {
			Time           time.Time `json:"time"`
			Name           string    `json:"name"`
			Variant        string    `json:"variant"`
			Chaos          float64   `json:"chaos"`
			Listings       int       `json:"listings"`
			IsTransfigured bool      `json:"isTransfigured"`
			IsCorrupted    bool      `json:"isCorrupted"`
			GemColor       string    `json:"gemColor"`
		}

		var results []gemRow
		for rows.Next() {
			var g gemRow
			if err := rows.Scan(&g.Time, &g.Name, &g.Variant, &g.Chaos, &g.Listings,
				&g.IsTransfigured, &g.IsCorrupted, &g.GemColor); err != nil {
				slog.Error("gem snapshots: scan row", "error", err)
				http.Error(w, `{"error":"scan failed"}`, http.StatusInternalServerError)
				return
			}
			results = append(results, g)
		}
		if err := rows.Err(); err != nil {
			slog.Error("gem snapshots: row iteration error", "error", err)
			http.Error(w, `{"error":"row iteration failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(results),
			"data":  results,
		}); err != nil {
			slog.Error("gem snapshots: encode response", "error", err)
		}
	}
}

// CurrencySnapshots returns currency snapshot data filtered by time range with pagination.
func CurrencySnapshots(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		q, err := parseSnapshotQuery(r)
		if err != nil {
			writeQueryError(w, "invalid query parameters", err)
			return
		}

		query := `SELECT time, currency_id, COALESCE(chaos, 0), COALESCE(sparkline_change, 0)
		          FROM currency_snapshots
		          WHERE time >= $1 AND time <= $2`
		args := []any{q.From, q.To}

		if q.Name != "" {
			query += ` AND currency_id = $3`
			args = append(args, q.Name)
			query += ` ORDER BY time DESC LIMIT $4 OFFSET $5`
			args = append(args, q.Limit, q.Offset)
		} else {
			query += ` ORDER BY time DESC, currency_id LIMIT $3 OFFSET $4`
			args = append(args, q.Limit, q.Offset)
		}

		rows, err := pool.Query(r.Context(), query, args...)
		if err != nil {
			slog.Error("currency snapshots: query failed", "error", err, "from", q.From, "to", q.To, "name", q.Name)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		defer rows.Close()

		type currRow struct {
			Time            time.Time `json:"time"`
			CurrencyID      string    `json:"currencyId"`
			Chaos           float64   `json:"chaos"`
			SparklineChange float64   `json:"sparklineChange"`
		}

		var results []currRow
		for rows.Next() {
			var c currRow
			if err := rows.Scan(&c.Time, &c.CurrencyID, &c.Chaos, &c.SparklineChange); err != nil {
				slog.Error("currency snapshots: scan row", "error", err)
				http.Error(w, `{"error":"scan failed"}`, http.StatusInternalServerError)
				return
			}
			results = append(results, c)
		}
		if err := rows.Err(); err != nil {
			slog.Error("currency snapshots: row iteration error", "error", err)
			http.Error(w, `{"error":"row iteration failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(results),
			"data":  results,
		}); err != nil {
			slog.Error("currency snapshots: encode response", "error", err)
		}
	}
}

// FragmentSnapshots returns fragment snapshot data filtered by time range with pagination.
func FragmentSnapshots(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		q, err := parseSnapshotQuery(r)
		if err != nil {
			writeQueryError(w, "invalid query parameters", err)
			return
		}

		query := `SELECT time, fragment_id, COALESCE(chaos, 0), COALESCE(sparkline_change, 0)
		          FROM fragment_snapshots
		          WHERE time >= $1 AND time <= $2`
		args := []any{q.From, q.To}

		if q.Name != "" {
			query += ` AND fragment_id = $3`
			args = append(args, q.Name)
			query += ` ORDER BY time DESC LIMIT $4 OFFSET $5`
			args = append(args, q.Limit, q.Offset)
		} else {
			query += ` ORDER BY time DESC, fragment_id LIMIT $3 OFFSET $4`
			args = append(args, q.Limit, q.Offset)
		}

		rows, err := pool.Query(r.Context(), query, args...)
		if err != nil {
			slog.Error("fragment snapshots: query failed", "error", err, "from", q.From, "to", q.To, "name", q.Name)
			http.Error(w, `{"error":"query failed"}`, http.StatusInternalServerError)
			return
		}
		defer rows.Close()

		type fragRow struct {
			Time            time.Time `json:"time"`
			FragmentID      string    `json:"fragmentId"`
			Chaos           float64   `json:"chaos"`
			SparklineChange float64   `json:"sparklineChange"`
		}

		var results []fragRow
		for rows.Next() {
			var f fragRow
			if err := rows.Scan(&f.Time, &f.FragmentID, &f.Chaos, &f.SparklineChange); err != nil {
				slog.Error("fragment snapshots: scan row", "error", err)
				http.Error(w, `{"error":"scan failed"}`, http.StatusInternalServerError)
				return
			}
			results = append(results, f)
		}
		if err := rows.Err(); err != nil {
			slog.Error("fragment snapshots: row iteration error", "error", err)
			http.Error(w, `{"error":"row iteration failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"count": len(results),
			"data":  results,
		}); err != nil {
			slog.Error("fragment snapshots: encode response", "error", err)
		}
	}
}

// SnapshotStats returns aggregate statistics about collected data.
func SnapshotStats(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		type stats struct {
			GemTotalRows      int       `json:"gemTotalRows"`
			GemSnapshotCount  int       `json:"gemSnapshotCount"`
			GemFirstSnapshot  time.Time `json:"gemFirstSnapshot"`
			GemLastSnapshot   time.Time `json:"gemLastSnapshot"`
			GemUniqueItems    int       `json:"gemUniqueItems"`
			CurrTotalRows     int       `json:"currTotalRows"`
			CurrSnapshotCount int       `json:"currSnapshotCount"`
			CurrFirstSnapshot time.Time `json:"currFirstSnapshot"`
			CurrLastSnapshot  time.Time `json:"currLastSnapshot"`
			CurrUniqueItems   int       `json:"currUniqueItems"`
			FragTotalRows     int       `json:"fragTotalRows"`
			FragSnapshotCount int       `json:"fragSnapshotCount"`
			FragFirstSnapshot time.Time `json:"fragFirstSnapshot"`
			FragLastSnapshot  time.Time `json:"fragLastSnapshot"`
			FragUniqueItems   int       `json:"fragUniqueItems"`
		}

		var s stats
		err := pool.QueryRow(r.Context(), `
			SELECT COUNT(*), COUNT(DISTINCT time),
			       COALESCE(MIN(time), '1970-01-01'::timestamptz),
			       COALESCE(MAX(time), '1970-01-01'::timestamptz),
			       COUNT(DISTINCT (name, variant, is_corrupted))
			FROM gem_snapshots`,
		).Scan(&s.GemTotalRows, &s.GemSnapshotCount, &s.GemFirstSnapshot, &s.GemLastSnapshot, &s.GemUniqueItems)
		if err != nil {
			slog.Error("snapshot stats: gem query failed", "error", err)
			http.Error(w, `{"error":"gem stats query failed"}`, http.StatusInternalServerError)
			return
		}

		err = pool.QueryRow(r.Context(), `
			SELECT COUNT(*), COUNT(DISTINCT time),
			       COALESCE(MIN(time), '1970-01-01'::timestamptz),
			       COALESCE(MAX(time), '1970-01-01'::timestamptz),
			       COUNT(DISTINCT currency_id)
			FROM currency_snapshots`,
		).Scan(&s.CurrTotalRows, &s.CurrSnapshotCount, &s.CurrFirstSnapshot, &s.CurrLastSnapshot, &s.CurrUniqueItems)
		if err != nil {
			slog.Error("snapshot stats: currency query failed", "error", err)
			http.Error(w, `{"error":"currency stats query failed"}`, http.StatusInternalServerError)
			return
		}

		err = pool.QueryRow(r.Context(), `
			SELECT COUNT(*), COUNT(DISTINCT time),
			       COALESCE(MIN(time), '1970-01-01'::timestamptz),
			       COALESCE(MAX(time), '1970-01-01'::timestamptz),
			       COUNT(DISTINCT fragment_id)
			FROM fragment_snapshots`,
		).Scan(&s.FragTotalRows, &s.FragSnapshotCount, &s.FragFirstSnapshot, &s.FragLastSnapshot, &s.FragUniqueItems)
		if err != nil {
			slog.Error("snapshot stats: fragment query failed", "error", err)
			http.Error(w, `{"error":"fragment stats query failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(s); err != nil {
			slog.Error("snapshot stats: encode response", "error", err)
		}
	}
}
