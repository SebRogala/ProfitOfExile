// Package useragent builds the User-Agent the server sends to GGG.
// docs/TRADE-LIFECYCLE.md § GGG policy is the normative source for its format.
package useragent

import (
	"os"
	"runtime/debug"
	"strings"
)

const (
	product        = "ProfitOfExile-server"
	contact        = "contact@profitofexile.top"
	unknownVersion = "unknown"
	revisionLength = 12
)

// Server returns "ProfitOfExile-server/<rev> (contact: …)", where <rev> is the
// runtime or binary revision cut to 12 characters, or "unknown" when unavailable.
func Server() string {
	return format(Revision(""))
}

// Revision resolves SOURCE_COMMIT, a legacy build stamp, then Go VCS metadata.
// The health handler passes its existing ldflags stamp; the collector has none.
func Revision(buildVersion string) string {
	info, ok := debug.ReadBuildInfo()
	return resolveRevision(os.Getenv("SOURCE_COMMIT"), buildVersion, info, ok)
}

func resolveRevision(sourceCommit, buildVersion string, info *debug.BuildInfo, ok bool) string {
	for _, value := range []string{sourceCommit, buildVersion} {
		if value = usableRevision(value); value != "" {
			return value
		}
	}
	return revision(info, ok)
}

func usableRevision(value string) string {
	value = strings.TrimSpace(value)
	switch strings.ToLower(value) {
	case "", "dev", "head", "unknown":
		return ""
	}
	return value
}

func format(rev string) string {
	if len(rev) > revisionLength {
		rev = rev[:revisionLength]
	}
	return product + "/" + rev + " (contact: " + contact + ")"
}

func revision(info *debug.BuildInfo, ok bool) string {
	if !ok || info == nil {
		return unknownVersion
	}
	for _, s := range info.Settings {
		if s.Key != "vcs.revision" {
			continue
		}
		if value := usableRevision(s.Value); value != "" {
			return value
		}
		return unknownVersion
	}
	return unknownVersion
}
