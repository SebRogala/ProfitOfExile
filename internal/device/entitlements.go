package device

// Update channels served to the desktop app. The channel selects which release
// manifest the updater consults; it is not an authorization boundary.
const (
	ChannelStable = "stable"
	ChannelBeta   = "beta"
)

// Hidden feature ids. Each gates visibility of one desktop feature: the
// mercenary triage module, the Currency Exchange page (server-backed, no
// desktop module), and the Temple of Atzoatl tools. The code behind every
// feature ships in every build — these only control whether the client
// renders it.
const (
	FeatureMerc     = "merc"
	FeatureExchange = "exchange"
	FeatureTemple   = "temple"
)

// Device roles. This is the whole role vocabulary: Entitlements switches on it
// and the promote CLI validates against it (cmd/promote), so a new role is
// added here and nowhere else. Roles are stored verbatim in the devices table,
// and matching is case-sensitive.
const (
	RoleUser   = "user"
	RoleEditor = "editor"
	RoleAdmin  = "admin"
)

// Entitlements maps a device role to its update channel and the hidden features
// it may see. Editor is the beta tier for now, so editor and admin get beta
// builds and every hidden module (merc, exchange, temple), and everything else
// — including an unknown role and a device the server has never identified —
// gets the stable channel with no hidden features.
//
// This is hiding, not security: the gate controls visibility and update
// channel, nothing more.
//
// features is always non-nil so the handler encodes it as [] rather than null;
// a nil there would make the desktop client's feature check depend on JSON
// null-handling instead of an empty list.
func Entitlements(role string) (channel string, features []string) {
	switch role {
	case RoleEditor, RoleAdmin:
		return ChannelBeta, []string{FeatureMerc, FeatureExchange, FeatureTemple}
	default:
		return ChannelStable, []string{}
	}
}
