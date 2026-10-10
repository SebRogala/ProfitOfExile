package tier

import (
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// Package tier is the one home of the price-tier algorithm: Lab, the Harvest
// generator and any later reader import it, so it imports nothing of theirs and
// nothing outside the standard library. An import of a caller would make the
// algorithm depend on one caller's model; a third-party import would put a
// dependency under every caller.

// algorithmFile must be among the parsed sources, so the guard cannot pass
// vacuously if the walk ever stops seeing them.
const algorithmFile = "tier.go"

func TestPackageSources_importOnlyTheStandardLibrary(t *testing.T) {
	entries, err := os.ReadDir(".")
	if err != nil {
		t.Fatalf("read package directory: %v", err)
	}

	fset := token.NewFileSet()
	parsed := make(map[string]bool)

	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() || !strings.HasSuffix(name, ".go") || strings.HasSuffix(name, "_test.go") {
			continue
		}

		file, err := parser.ParseFile(fset, filepath.Join(".", name), nil, parser.ImportsOnly)
		if err != nil {
			t.Fatalf("parse %s: %v", name, err)
		}
		parsed[name] = true

		for _, spec := range file.Imports {
			path, err := strconv.Unquote(spec.Path.Value)
			if err != nil {
				t.Fatalf("%s: unquote import %s: %v", name, spec.Path.Value, err)
			}
			if strings.HasPrefix(path, "profitofexile/") {
				t.Errorf("%s imports %q: internal/price/tier must not import a profitofexile package — "+
					"callers map their own model to tier.Item", name, path)
			}
			if first, _, _ := strings.Cut(path, "/"); strings.Contains(first, ".") {
				t.Errorf("%s imports %q: internal/price/tier imports the standard library only", name, path)
			}
		}
	}

	if !parsed[algorithmFile] {
		t.Fatalf("%s was not parsed (parsed %d files) — the import guard would pass vacuously", algorithmFile, len(parsed))
	}
}
