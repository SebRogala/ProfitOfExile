package useragent

import (
	"runtime/debug"
	"strings"
	"testing"
)

func TestVCSUserAgent(t *testing.T) {
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
			if got := format(revision(tt.info, tt.ok)); got != tt.want {
				t.Errorf("User-Agent = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestResolveRevision(t *testing.T) {
	const source = "fedcba9876543210fedcba9876543210fedcba98"
	const stamped = "abcdef0123456789abcdef0123456789abcdef01"
	const vcs = "0123456789abcdef0123456789abcdef01234567"
	info := &debug.BuildInfo{Settings: []debug.BuildSetting{{Key: "vcs.revision", Value: vcs}}}
	tests := []struct {
		name   string
		source string
		stamp  string
		info   *debug.BuildInfo
		ok     bool
		want   string
	}{
		{"runtime source precedes build stamp", source, stamped, info, true, source},
		{"build stamp precedes vcs", "", stamped, info, true, stamped},
		{"vcs fallback retains full revision", "", "dev", info, true, vcs},
		{"missing metadata yields unknown", "", "dev", nil, false, "unknown"},
		{"runtime whitespace trimmed", " \t" + source + "\n", stamped, info, true, source},
		{"empty runtime falls back", " \t", stamped, info, true, stamped},
		{"dev runtime falls back", "dev", stamped, info, true, stamped},
		{"head runtime falls back", "HEAD", stamped, info, true, stamped},
		{"unknown runtime falls back", "unknown", stamped, info, true, stamped},
		{"head stamp falls back to vcs", "", "HEAD", info, true, vcs},
		{"unknown stamp falls back to vcs", "", "unknown", info, true, vcs},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := resolveRevision(tt.source, tt.stamp, tt.info, tt.ok); got != tt.want {
				t.Errorf("revision = %q, want %q", got, tt.want)
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
