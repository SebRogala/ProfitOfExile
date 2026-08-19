package exchange

import (
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// forbiddenImport is the package internal/exchange must never reach into.
//
// The rule is a layering decision the compiler cannot catch: internal/collector
// does not import internal/exchange, so an import in this direction compiles
// fine and only shows up as a cycle once the collector grows one back. The
// Mercure stamping adapter that legitimately needs both lives in cmd/collector
// (see internal/exchange/doc.go and docs/adr/008-current-go-package-architecture.md).
const forbiddenImport = "profitofexile/internal/collector"

// lifecycleFiles are the files most likely to reach for the collector: they are
// the database and scheduling layer. Requiring them to be among the parsed files
// keeps the guard from passing vacuously if the walk ever stops seeing sources.
var lifecycleFiles = []string{"repository.go", "runner.go"}

func TestPackageSources_neverImportInternalCollector(t *testing.T) {
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
			if path == forbiddenImport || strings.HasPrefix(path, forbiddenImport+"/") {
				t.Errorf("%s imports %q: internal/exchange must not import internal/collector — "+
					"put the code that needs both in cmd/collector instead", name, path)
			}
		}
	}

	for _, name := range lifecycleFiles {
		if !parsed[name] {
			t.Fatalf("%s was not parsed (parsed %d files) — the import guard would pass vacuously", name, len(parsed))
		}
	}
}
