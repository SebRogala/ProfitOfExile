package mercenary

import (
	"time"
)

// Default write budget for one device: 60 requests per 10 minutes.
//
// Sized against the real corpus rather than against a guess. A device holding
// the full local store (459 keys, up to 1377 signatures — families x tiers x
// MaxSamplesPerKey, see lockPoolSQL) needs 22 requests to publish everything at
// the server's own MaxTemplatesPerUpload cap, or 43 at the 32 per batch the
// desktop actually sends; a device that is merely learning sends one request per
// new hover. So the worst realistic publish leaves 17 of the 60 for a session's
// hovers — headroom, but not much of it, and a client that batched below 32
// would run out. 60 still bounds a spoofed fingerprint to 60 requests before it
// has to wait.
const (
	DefaultUploadBudget = 60
	DefaultUploadWindow = 10 * time.Minute
	// DefaultRateLimitDevices bounds the bucket map, the same safety valve the
	// device middleware puts on its cache: a flood of distinct well-formed
	// fingerprints must not grow memory without limit.
	DefaultRateLimitDevices = 8192
)
