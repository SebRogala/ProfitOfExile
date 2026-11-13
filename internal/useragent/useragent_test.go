package useragent

import (
	"runtime/debug"
	"strings"
	"testing"
)

func TestFromBuildInfo(t *testing.T) {
	const fullRev = "0123456789abcdef0123456789abcdef01234567"
	const unknownUA = "ProfitOfExile-server/unknown (contact: contact@profitofexile.top)"
	settings := func(kv ...string) *debug.BuildInfo {
		info := &debug.BuildInfo{}
		for i := 0; i < len(kv); i += 2 {
			info.Settings = append(info.Settings, debug.BuildSetting{Key: kv[i], Value: kv[i+1]})
		}
		return info
	}

	tests := []struct {
		name string
		info *debug.BuildInfo
		ok   bool
		want string
	}{
		{"full revision is cut to 12 characters", settings("vcs", "git", "vcs.revision", fullRev), true,
			"ProfitOfExile-server/0123456789ab (contact: contact@profitofexile.top)"},
		{"short revision is used as is", settings("vcs.revision", "abc1234"), true,
			"ProfitOfExile-server/abc1234 (contact: contact@profitofexile.top)"},
		{"read failure yields unknown", settings("vcs.revision", fullRev), false, unknownUA},
		{"nil info yields unknown", nil, true, unknownUA},
		{"missing revision yields unknown", settings("vcs", "git", "vcs.time", "2026-11-13T00:00:00Z"), true, unknownUA},
		{"empty revision yields unknown", settings("vcs.revision", ""), true, unknownUA},
		{"modified tree adds no suffix", settings("vcs.revision", fullRev, "vcs.modified", "true"), true,
			"ProfitOfExile-server/0123456789ab (contact: contact@profitofexile.top)"},
		{"module version is never used", &debug.BuildInfo{Main: debug.Module{Version: "v1.2.3"}}, true, unknownUA},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := fromBuildInfo(tt.info, tt.ok); got != tt.want {
				t.Errorf("fromBuildInfo() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestServer_hasProductPrefixAndContact(t *testing.T) {
	got := Server()
	if !strings.HasPrefix(got, "ProfitOfExile-server/") || !strings.HasSuffix(got, " (contact: contact@profitofexile.top)") {
		t.Errorf("Server() = %q, want ProfitOfExile-server/<rev> (contact: contact@profitofexile.top)", got)
	}
}
