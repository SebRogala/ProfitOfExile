package device

import "time"

// Device represents a registered desktop app instance identified by a
// hardware-derived fingerprint. Devices are auto-registered on first request
// and tracked via the X-Device-ID header.
type Device struct {
	Fingerprint string    `json:"fingerprint"`
	Alias       *string   `json:"alias"`
	Role        string    `json:"role"`
	Banned      bool      `json:"banned"`
	AppVersion  *string   `json:"app_version"`
	FirstSeen   time.Time `json:"first_seen"`
	LastSeen    time.Time `json:"last_seen"`
}

// IsIdentified returns true when the device has an alias set.
func (d *Device) IsIdentified() bool {
	return d.Alias != nil && *d.Alias != ""
}
