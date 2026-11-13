// Package useragent builds the User-Agent the server sends to GGG.
// docs/TRADE-LIFECYCLE.md § GGG policy is the normative source for its format.
package useragent

import (
	"runtime/debug"
	"sync"
)

const (
	product        = "ProfitOfExile-server"
	contact        = "contact@profitofexile.top"
	unknownVersion = "unknown"
	revisionLength = 12
)

// Server returns "ProfitOfExile-server/<rev> (contact: …)", where <rev> is the
// binary's VCS revision cut to 12 characters, or "unknown" when none is stamped.
var Server = sync.OnceValue(func() string {
	return fromBuildInfo(debug.ReadBuildInfo())
})

func fromBuildInfo(info *debug.BuildInfo, ok bool) string {
	return product + "/" + revision(info, ok) + " (contact: " + contact + ")"
}

func revision(info *debug.BuildInfo, ok bool) string {
	if !ok || info == nil {
		return unknownVersion
	}
	for _, s := range info.Settings {
		if s.Key != "vcs.revision" {
			continue
		}
		if s.Value == "" {
			return unknownVersion
		}
		if len(s.Value) > revisionLength {
			return s.Value[:revisionLength]
		}
		return s.Value
	}
	return unknownVersion
}
