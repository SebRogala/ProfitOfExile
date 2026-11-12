package handlers

import (
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strconv"
	"strings"
	"testing"
)

// forbiddenImportPrefixes are the persistence and ingest packages handlers must
// not reach into: SQL belongs in repositories (internal/lab, internal/bossloot,
// ...) and the collector lifecycle in cmd/collector. A path matches when it
// equals a prefix or starts with prefix + "/", so pgxpool and pgconn are covered.
var forbiddenImportPrefixes = []string{
	"github.com/jackc/pgx/v5",
	"profitofexile/internal/collector",
}

// importBaseline lists the files that still import a forbidden path, each with
// the task that removes it. The baseline ratchets: an entry whose file no longer
// imports a forbidden path fails the build until it is removed.
var importBaseline = map[string]string{
	"analysis.go":     "POE-339: divine-rate fallback query in AnalysisStatus",
	"boss_loot.go":    "POE-344: NewBossLoot builds its repository from the pool",
	"debug.go":        "server audit L1: debug endpoints read collector state",
	"font_session.go": "POE-344: font-session insert transaction",
}

// guardFiles must be among the parsed files so the walk cannot pass vacuously.
// health.go mentions pgxpool only in a comment, so its absence from any
// violation also shows the parse reads imports only.
var guardFiles = []string{"analysis.go", "health.go"}

func isForbiddenImport(path string) bool {
	for _, prefix := range forbiddenImportPrefixes {
		if path == prefix || strings.HasPrefix(path, prefix+"/") {
			return true
		}
	}
	return false
}

// forbiddenImportsBySource parses every non-test Go file in this directory and
// returns each file's forbidden imports (files with none map to an empty slice).
func forbiddenImportsBySource(t *testing.T) map[string][]string {
	t.Helper()
	entries, err := os.ReadDir(".")
	if err != nil {
		t.Fatalf("read package directory: %v", err)
	}
	fset := token.NewFileSet()
	result := make(map[string][]string)
	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() || !strings.HasSuffix(name, ".go") || strings.HasSuffix(name, "_test.go") {
			continue
		}
		file, err := parser.ParseFile(fset, filepath.Join(".", name), nil, parser.ImportsOnly)
		if err != nil {
			t.Fatalf("parse %s: %v", name, err)
		}
		forbidden := []string{}
		for _, spec := range file.Imports {
			path, err := strconv.Unquote(spec.Path.Value)
			if err != nil {
				t.Fatalf("%s: unquote import %s: %v", name, spec.Path.Value, err)
			}
			if isForbiddenImport(path) {
				forbidden = append(forbidden, path)
			}
		}
		result[name] = forbidden
	}
	for _, name := range guardFiles {
		if _, ok := result[name]; !ok {
			t.Fatalf("%s was not parsed (parsed %d files) — the import guard would pass vacuously", name, len(result))
		}
	}
	return result
}

func TestForbiddenImportPrefixes_namesPgxAndCollector(t *testing.T) {
	want := []string{"github.com/jackc/pgx/v5", "profitofexile/internal/collector"}
	if !reflect.DeepEqual(forbiddenImportPrefixes, want) {
		t.Errorf("forbiddenImportPrefixes = %v, want %v", forbiddenImportPrefixes, want)
	}
}

func TestHandlerSources_importNoPersistenceOrCollectorOutsideBaseline(t *testing.T) {
	sources := forbiddenImportsBySource(t)
	names := make([]string, 0, len(sources))
	for name := range sources {
		names = append(names, name)
	}
	sort.Strings(names)
	for _, name := range names {
		if _, baselined := importBaseline[name]; baselined {
			continue
		}
		for _, path := range sources[name] {
			t.Errorf("%s imports %q: handlers must not import persistence or collector packages — "+
				"move the query into a repository and inject it", name, path)
		}
	}
}

func TestImportBaseline_holdsOnlyFilesThatStillImportAForbiddenPath(t *testing.T) {
	sources := forbiddenImportsBySource(t)
	names := make([]string, 0, len(importBaseline))
	for name := range importBaseline {
		names = append(names, name)
	}
	sort.Strings(names)
	for _, name := range names {
		imports, exists := sources[name]
		if !exists {
			t.Errorf("baseline entry %s no longer exists — remove it from importBaseline", name)
			continue
		}
		if len(imports) == 0 {
			t.Errorf("baseline entry %s no longer imports a forbidden path — remove it from importBaseline", name)
		}
	}
}
