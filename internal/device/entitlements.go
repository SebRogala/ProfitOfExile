package device

// Update channels served to the desktop app. The channel selects which release
// manifest the updater consults; it is not an authorization boundary.
const (
	ChannelStable = "stable"
	ChannelBeta   = "beta"
)

// Feature ids control desktop visibility. Temple is granted to every device;
// mercenary triage, Currency Exchange (server-backed, no desktop module), and
// beta features (the desktop's BetaGate) stay hidden for stable, unknown, and
// unidentified devices. The code behind every feature ships in every build —
// these only control whether the client renders it.
const (
	FeatureMerc     = "merc"
	FeatureExchange = "exchange"
	FeatureTemple   = "temple"
	FeatureBeta     = "beta"
)

// Device roles. This is the whole role vocabulary: Entitlements switches on it
// and the promote CLI validates against it (cmd/promote), so a new role is
// added here and nowhere else. Roles are stored verbatim in the devices table,
// and matching is case-sensitive.
const (
	RoleUser   = "user"
	RoleBeta   = "beta"
	RoleEditor = "editor"
	RoleAdmin  = "admin"
)

// Entitlements maps a device role to its update channel and visible features.
// Every device gets temple. Beta, editor and admin get beta builds and all four
// features (merc, exchange, temple, beta) — editor and admin include beta access.
// Everything else, including unknown roles and unidentified devices, gets the
// stable channel with only temple; merc, exchange and beta stay hidden.
//
// This is hiding, not security: the gate controls visibility and update
// channel, nothing more.
//
// features is always non-nil so the handler encodes an array rather than null.
// Each call returns a fresh slice so callers cannot change later answers.
func Entitlements(role string) (channel string, features []string) {
	switch role {
	case RoleBeta, RoleEditor, RoleAdmin:
		return ChannelBeta, []string{FeatureMerc, FeatureExchange, FeatureTemple, FeatureBeta}
	default:
		return ChannelStable, []string{FeatureTemple}
	}
}
