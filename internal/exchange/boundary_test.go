package exchange

import (
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"reflect"
	"strconv"
	"strings"
	"testing"
)

// forbiddenImports are the packages internal/exchange must never reach into.
//
// The rule is a layering decision the compiler cannot catch: neither package
// imports internal/exchange, so an import in this direction compiles fine and
// only shows up as a cycle once one of them grows an import back.
//
// internal/collector is the ingest lifecycle this package deliberately parallels
// rather than joins; the Mercure stamping adapter that legitimately needs both
// lives in cmd/collector. internal/lab is the gem-farming analysis stack — the
// best-plays engine scores the currency feed on its own terms and must not
// borrow lab's scoring, tiering or signal vocabulary (see
// internal/exchange/doc.go and docs/adr/008-current-go-package-architecture.md).
var forbiddenImports = []string{
	"profitofexile/internal/collector",
	"profitofexile/internal/lab",
}

// lifecycleFiles are the files most likely to reach for the collector: they are
// the database and scheduling layer. Requiring them to be among the parsed files
// keeps the guard from passing vacuously if the walk ever stops seeing sources.
var lifecycleFiles = []string{"repository.go", "runner.go"}

func TestForbiddenImports_namesBothLayeringRules(t *testing.T) {
	// The list is what the walk below enforces, so dropping an entry would
	// silently retire a rule while the guard kept passing.
	want := []string{"profitofexile/internal/collector", "profitofexile/internal/lab"}

	if !reflect.DeepEqual(forbiddenImports, want) {
		t.Errorf("forbiddenImports = %v, want %v", forbiddenImports, want)
	}
}

func TestPackageSources_neverImportTheForbiddenPackages(t *testing.T) {
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
			for _, forbidden := range forbiddenImports {
				if path == forbidden || strings.HasPrefix(path, forbidden+"/") {
					t.Errorf("%s imports %q: internal/exchange must not import %s — "+
						"put the code that needs both in a caller such as cmd/collector instead",
						name, path, forbidden)
				}
			}
		}
	}

	for _, name := range lifecycleFiles {
		if !parsed[name] {
			t.Fatalf("%s was not parsed (parsed %d files) — the import guard would pass vacuously", name, len(parsed))
		}
	}
}
