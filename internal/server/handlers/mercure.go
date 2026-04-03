package handlers

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"log/slog"
	"net/http"
	"time"
)

// MercureToken returns a short-lived subscriber JWT and the public Mercure URL.
// The frontend uses this to connect to Mercure SSE directly from the browser.
func MercureToken(subscriberKey, publicURL string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if subscriberKey == "" || publicURL == "" {
			http.Error(w, `{"error":"Mercure not configured"}`, http.StatusServiceUnavailable)
			return
		}

		token, err := signSubscriberToken(subscriberKey)
		if err != nil {
			slog.Error("mercure token: sign failed", "error", err)
			http.Error(w, `{"error":"token generation failed"}`, http.StatusInternalServerError)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]string{
			"token": token,
			"url":   publicURL,
		}); err != nil {
			slog.Error("mercure token: encode response", "error", err)
		}
	}
}

func signSubscriberToken(secret string) (string, error) {
	header := base64.RawURLEncoding.EncodeToString([]byte(`{"alg":"HS256","typ":"JWT"}`))

	now := time.Now()
	claims := map[string]any{
		"mercure": map[string]any{
			"subscribe": []string{
				"poe/analysis/updated",
				"poe/trade/results",
				"poe/desktop/{pair}",
				"poe/lab/layout",
			},
		},
		"iat": now.Unix(),
		"exp": now.Add(30 * time.Minute).Unix(),
	}
	claimsJSON, err := json.Marshal(claims)
	if err != nil {
		return "", err
	}
	payload := base64.RawURLEncoding.EncodeToString(claimsJSON)

	unsigned := header + "." + payload
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(unsigned))
	signature := base64.RawURLEncoding.EncodeToString(mac.Sum(nil))

	return unsigned + "." + signature, nil
}
