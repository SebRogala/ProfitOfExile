package device

// Update channels served to the desktop app. The channel selects which release
// manifest the updater consults; it is not an authorization boundary.
const (
	ChannelStable = "stable"
	ChannelBeta   = "beta"
)

// Hidden feature ids. Each gates visibility of one desktop feature: the
// mercenary triage module, the Currency Exchange page (server-backed, no
// desktop module), the Temple of Atzoatl tools, and the settings marked beta
// inside a visible module (the desktop's BetaGate). The code behind every
// feature ships in every build — these only control whether the client
// renders it.
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

// Entitlements maps a device role to its update channel and the hidden features
// it may see. Beta, editor and admin all get beta builds, every hidden module
// (merc, exchange, temple) and the beta settings — editor and admin include
// beta access. Everything else — including an unknown role and a device the
// server has never identified — gets the stable channel with no hidden
// features.
//
// This is hiding, not security: the gate controls visibility and update
// channel, nothing more.
//
// features is always non-nil so the handler encodes it as [] rather than null;
// a nil there would make the desktop client's feature check depend on JSON
// null-handling instead of an empty list.
func Entitlements(role string) (channel string, features []string) {
	switch role {
	case RoleBeta, RoleEditor, RoleAdmin:
		return ChannelBeta, []string{FeatureMerc, FeatureExchange, FeatureTemple, FeatureBeta}
	default:
		return ChannelStable, []string{}
	}
}
