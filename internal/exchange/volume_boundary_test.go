package exchange

import (
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// minVolumeGuardFiles keeps the walk below from passing vacuously if it ever
// stops seeing sources; internal/ and cmd/ held 298 such files outside
// internal/exchange when the guard landed.
const minVolumeGuardFiles = 250

// moduleRoot walks up from the package directory to the directory holding go.mod.
func moduleRoot(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatalf("no go.mod above the package directory")
		}
		dir = parent
	}
}

func TestVolumeFields_areReadOnlyInsideTheExchangePackage(t *testing.T) {
	// Which stored volume belongs to which side of a market is decided only by
	// pricing.go's direction mappers; a raw VolumeA/VolumeB read elsewhere is a
	// second copy of that mapping that can drift (POE-340). Struct-literal keys
	// are not selector reads, so test fixtures that build rows stay legal.
	root := moduleRoot(t)
	exchangeDir := filepath.Join(root, "internal", "exchange")
	fset := token.NewFileSet()
	parsed := 0

	for _, top := range []string{"internal", "cmd"} {
		err := filepath.WalkDir(filepath.Join(root, top), func(path string, d fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() {
				if path == exchangeDir {
					return filepath.SkipDir
				}
				return nil
			}
			if !strings.HasSuffix(path, ".go") {
				return nil
			}
			file, err := parser.ParseFile(fset, path, nil, parser.SkipObjectResolution)
			if err != nil {
				return err
			}
			parsed++
			ast.Inspect(file, func(n ast.Node) bool {
				sel, ok := n.(*ast.SelectorExpr)
				if ok && (sel.Sel.Name == "VolumeA" || sel.Sel.Name == "VolumeB") {
					pos := fset.Position(sel.Sel.Pos())
					rel, _ := filepath.Rel(root, pos.Filename)
					t.Errorf("%s:%d reads %s: price from exchange.PooledVWAP or another pricing.go reader instead",
						rel, pos.Line, sel.Sel.Name)
				}
				return true
			})
			return nil
		})
		if err != nil {
			t.Fatalf("walk %s: %v", top, err)
		}
	}

	if parsed < minVolumeGuardFiles {
		t.Fatalf("parsed %d .go files under internal/ and cmd/, want at least %d", parsed, minVolumeGuardFiles)
	}
}
