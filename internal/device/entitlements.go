package device

// Update channels served to the desktop app. The channel selects which release
// manifest the updater consults; it is not an authorization boundary.
const (
	ChannelStable = "stable"
	ChannelBeta   = "beta"
)

// FeatureMerc gates visibility of the mercenary triage module in the desktop
// app. The module's code ships in every build — this only controls whether the
// client renders it.
const FeatureMerc = "merc"

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
// builds and the merc module and everything else — including an unknown role
// and a device the server has never identified — gets the stable channel with
// no hidden features.
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
		return ChannelBeta, []string{FeatureMerc}
	default:
		return ChannelStable, []string{}
	}
}
