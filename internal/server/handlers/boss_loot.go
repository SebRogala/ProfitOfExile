package handlers

import (
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"strconv"

	"github.com/go-chi/chi/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"profitofexile/internal/bossloot"
	"profitofexile/internal/league"
	"profitofexile/internal/server/middleware"
)

type BossLoot struct {
	repo *bossloot.Repository
	pool *pgxpool.Pool
}

func NewBossLoot(pool *pgxpool.Pool) *BossLoot {
	return &BossLoot{repo: bossloot.NewRepository(pool), pool: pool}
}
func bossDevice(w http.ResponseWriter, r *http.Request) string {
	dev := middleware.DeviceFromContext(r.Context())
	if dev == nil {
		jsonError(w, 401, "device identification required")
		return ""
	}
	return dev.Fingerprint
}
func bossDecode(w http.ResponseWriter, r *http.Request, body any) bool {
	r.Body = http.MaxBytesReader(w, r.Body, 1<<20)
	decoder := json.NewDecoder(r.Body)
	if err := decoder.Decode(body); err != nil {
		jsonError(w, 400, "invalid JSON body")
		return false
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		jsonError(w, 400, "expected one JSON body")
		return false
	}
	return true
}
func bossRespond(w http.ResponseWriter, status int, body any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	if err := json.NewEncoder(w).Encode(body); err != nil {
		slog.Warn("boss loot: encode response", "error", err)
	}
}
func bossError(w http.ResponseWriter, err error) {
	switch {
	case errors.Is(err, bossloot.ErrInvalid):
		jsonError(w, 400, err.Error())
	case errors.Is(err, bossloot.ErrNotFound):
		jsonError(w, 404, "boss loot run not found")
	case errors.Is(err, bossloot.ErrConflict):
		jsonError(w, 409, "boss loot version or merge target conflict")
	default:
		slog.Error("boss loot request failed", "error", err)
		jsonError(w, 500, "boss loot request failed")
	}
}
func bossProfile(r *http.Request) (bossloot.Profile, error) {
	q := r.URL.Query()
	v, err := strconv.Atoi(q.Get("profileVersion"))
	if err != nil {
		return bossloot.Profile{}, bossloot.ErrInvalid
	}
	p := bossloot.Profile{Boss: q.Get("boss"), ProfileKey: q.Get("profileKey"), ProfileVersion: v}
	return p, p.Validate()
}
func (h *BossLoot) Submit(w http.ResponseWriter, r *http.Request) {
	device := bossDevice(w, r)
	if device == "" {
		return
	}
	var body bossloot.Submit
	if !bossDecode(w, r, &body) {
		return
	}
	if err := body.Validate(); err != nil {
		bossError(w, err)
		return
	}
	scope, err := league.Resolve(r.Context(), h.pool)
	if err != nil {
		bossError(w, err)
		return
	}
	out, err := h.repo.Submit(r.Context(), scope, device, body)
	if err != nil {
		bossError(w, err)
		return
	}
	status := 201
	if body.Mode == "merge" {
		status = 200
	}
	bossRespond(w, status, out)
}
func (h *BossLoot) List(w http.ResponseWriter, r *http.Request) {
	device := bossDevice(w, r)
	if device == "" {
		return
	}
	profile, err := bossProfile(r)
	if err != nil {
		bossError(w, err)
		return
	}
	page, size := 1, 50
	q := r.URL.Query()
	if q.Has("page") {
		page, err = strconv.Atoi(q.Get("page"))
		if err != nil {
			bossError(w, bossloot.ErrInvalid)
			return
		}
	}
	if q.Has("pageSize") {
		size, err = strconv.Atoi(q.Get("pageSize"))
		if err != nil {
			bossError(w, bossloot.ErrInvalid)
			return
		}
	}
	if err = bossloot.ValidatePagination(page, size); err != nil {
		bossError(w, err)
		return
	}
	out, err := h.repo.ListCumulative(r.Context(), device, profile, page, size)
	if err != nil {
		bossError(w, err)
		return
	}
	bossRespond(w, 200, out)
}
func (h *BossLoot) Stats(w http.ResponseWriter, r *http.Request) {
	device := bossDevice(w, r)
	if device == "" {
		return
	}
	profile, err := bossProfile(r)
	if err != nil {
		bossError(w, err)
		return
	}
	scope := r.URL.Query().Get("scope")
	if scope != "own" && scope != "collective" {
		bossError(w, bossloot.ErrInvalid)
		return
	}
	out, err := h.repo.StatsCumulative(r.Context(), device, profile, scope == "collective")
	if err != nil {
		bossError(w, err)
		return
	}
	bossRespond(w, 200, out)
}
func (h *BossLoot) Edit(w http.ResponseWriter, r *http.Request) {
	device := bossDevice(w, r)
	if device == "" {
		return
	}
	var body struct {
		ExpectedVersion int64          `json:"expectedVersion"`
		Items           bossloot.Items `json:"items"`
	}
	if !bossDecode(w, r, &body) {
		return
	}
	id := chi.URLParam(r, "id")
	if err := bossloot.ValidateTarget(id, body.ExpectedVersion); err != nil {
		bossError(w, err)
		return
	}
	out, err := h.repo.EditOwned(r.Context(), device, id, body.ExpectedVersion, body.Items)
	if err != nil {
		bossError(w, err)
		return
	}
	bossRespond(w, 200, out)
}
func (h *BossLoot) Delete(w http.ResponseWriter, r *http.Request) {
	device := bossDevice(w, r)
	if device == "" {
		return
	}
	var body struct {
		ExpectedVersion int64 `json:"expectedVersion"`
	}
	if !bossDecode(w, r, &body) {
		return
	}
	id := chi.URLParam(r, "id")
	if err := bossloot.ValidateTarget(id, body.ExpectedVersion); err != nil {
		bossError(w, err)
		return
	}
	if err := h.repo.DeleteOwned(r.Context(), device, id, body.ExpectedVersion); err != nil {
		bossError(w, err)
		return
	}
	bossRespond(w, 200, map[string]any{"deleted": true, "id": id})
}
