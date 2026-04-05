package device

import (
	"context"
	"time"
)

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

// Upserter can register or refresh a device by fingerprint.
// Implemented by Repository; used by the device middleware.
type Upserter interface {
	Upsert(ctx context.Context, fingerprint, appVersion string) (*Device, error)
}

// Lister can list all registered devices.
// Implemented by Repository; used by the admin devices handler.
type Lister interface {
	List(ctx context.Context) ([]Device, error)
}

// AliasSetter can update a device alias.
// Implemented by Repository; used by the identify handler.
type AliasSetter interface {
	SetAlias(ctx context.Context, fingerprint, alias string) error
}
