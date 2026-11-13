package book

import (
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

// The book is the engine's service layer: readers (lab, server, cmd) import
// it, so it imports none of them. It sees the stores only through its ports.
var allowedProjectImports = map[string]bool{
	"profitofexile/internal/price/resolve": true,
	"profitofexile/internal/league":        true,
}

// bookFile must be among the parsed sources, so the guard cannot pass
// vacuously if the walk ever stops seeing them.
const bookFile = "book.go"

func TestPackageSources_importOnlyTheResolverLeagueAndStandardLibrary(t *testing.T) {
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
				if !allowedProjectImports[path] {
					t.Errorf("%s imports %q: internal/price/book imports only internal/price/resolve and internal/league", name, path)
				}
				continue
			}
			if first, _, _ := strings.Cut(path, "/"); strings.Contains(first, ".") {
				t.Errorf("%s imports %q: internal/price/book imports no third-party package", name, path)
			}
		}
	}

	if !parsed[bookFile] {
		t.Fatalf("%s was not parsed (parsed %d files) — the import guard would pass vacuously", bookFile, len(parsed))
	}
}
