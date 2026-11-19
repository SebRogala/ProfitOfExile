package mercure

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"time"
)

// SignJWT creates an HS256-signed Mercure JWT carrying claim as its "mercure"
// claim, issued now and expiring after ttl. The secret is the HMAC-SHA256 key.
func SignJWT(secret string, claim map[string]any, ttl time.Duration) (string, error) {
	return signJWTAt(secret, claim, ttl, time.Now())
}

func signJWTAt(secret string, claim map[string]any, ttl time.Duration, now time.Time) (string, error) {
	header := base64.RawURLEncoding.EncodeToString([]byte(`{"alg":"HS256","typ":"JWT"}`))

	claims := map[string]any{
		"mercure": claim,
		"iat":     now.Unix(),
		"exp":     now.Add(ttl).Unix(),
	}
	claimsJSON, err := json.Marshal(claims)
	if err != nil {
		return "", fmt.Errorf("marshal JWT claims: %w", err)
	}
	payload := base64.RawURLEncoding.EncodeToString(claimsJSON)

	unsigned := header + "." + payload
	mac := hmac.New(sha256.New, []byte(secret))
	mac.Write([]byte(unsigned))
	signature := base64.RawURLEncoding.EncodeToString(mac.Sum(nil))

	return unsigned + "." + signature, nil
}
