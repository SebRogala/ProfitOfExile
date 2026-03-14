package handlers

import (
	"encoding/json"
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
			return nil, err
		}
		q.From = t
	}
	if v := r.URL.Query().Get("to"); v != "" {
		t, err := time.Parse(time.RFC3339, v)
		if err != nil {
			return nil, err
		}
		q.To = t
	}
	if v := r.URL.Query().Get("name"); v != "" {
		q.Name = v
	}
	if v := r.URL.Query().Get("limit"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 1 {
			return nil, err
		}
		if n > 10000 {
			n = 10000
		}
		q.Limit = n
	}
	if v := r.URL.Query().Get("offset"); v != "" {
		n, err := strconv.Atoi(v)
		if err != nil || n < 0 {
			return nil, err
		}
		q.Offset = n
	}

	return q, nil
}

// GemSnapshots returns gem snapshot data filtered by time range, name, with pagination.
func GemSnapshots(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		q, err := parseSnapshotQuery(r)
		if err != nil {
			http.Error(w, `{"error":"invalid query parameters: `+err.Error()+`"}`, http.StatusBadRequest)
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
				http.Error(w, `{"error":"scan failed"}`, http.StatusInternalServerError)
				return
			}
			results = append(results, g)
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"count": len(results),
			"data":  results,
		})
	}
}

// CurrencySnapshots returns currency snapshot data filtered by time range with pagination.
func CurrencySnapshots(pool *pgxpool.Pool) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		q, err := parseSnapshotQuery(r)
		if err != nil {
			http.Error(w, `{"error":"invalid query parameters: `+err.Error()+`"}`, http.StatusBadRequest)
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
				http.Error(w, `{"error":"scan failed"}`, http.StatusInternalServerError)
				return
			}
			results = append(results, c)
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"count": len(results),
			"data":  results,
		})
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
			http.Error(w, `{"error":"currency stats query failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(s)
	}
}
