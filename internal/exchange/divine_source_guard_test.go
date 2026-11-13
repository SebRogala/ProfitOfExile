package exchange

import (
	"go/ast"
	"go/parser"
	"go/token"
	"io/fs"
	"path/filepath"
	"strings"
	"testing"
)

// divineChaosFuncs compute a pair's VWAP from stored rows. Called with the
// divine/chaos pair they are a divine→chaos rate, whose one home outside this
// package is DivineRateReader.
var divineChaosFuncs = map[string]bool{"PooledVWAP": true, "NewestHourVWAP": true, "vwapIn": true}

func TestDivineChaosVWAP_isCalledOnlyInsideExchange(t *testing.T) {
	const root = "../.."
	var found []string
	parsedHarvest := false
	for _, dir := range []string{"cmd", "internal"} {
		err := filepath.WalkDir(filepath.Join(root, dir), func(path string, d fs.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() || !strings.HasSuffix(path, ".go") || strings.HasSuffix(path, "_test.go") {
				return nil
			}
			rel, err := filepath.Rel(root, path)
			if err != nil {
				return err
			}
			rel = filepath.ToSlash(rel)
			if strings.HasPrefix(rel, "internal/exchange/") {
				return nil
			}
			file, err := parser.ParseFile(token.NewFileSet(), path, nil, 0)
			if err != nil {
				return err
			}
			if rel == "internal/harvest/prices.go" {
				parsedHarvest = true
			}
			ast.Inspect(file, func(n ast.Node) bool {
				call, ok := n.(*ast.CallExpr)
				if !ok || len(call.Args) < 3 || !divineChaosFuncs[calleeName(call.Fun)] {
					return true
				}
				a, b := selectorName(call.Args[1]), selectorName(call.Args[2])
				if (a == "DivineID" && b == "ChaosID") || (a == "ChaosID" && b == "DivineID") {
					found = append(found, rel)
				}
				return true
			})
			return nil
		})
		if err != nil {
			t.Fatalf("walk %s: %v", dir, err)
		}
	}

	if !parsedHarvest {
		t.Fatal("the walk never parsed internal/harvest/prices.go, so the guard checked nothing")
	}
	if len(found) != 0 {
		t.Errorf("divine/chaos VWAP computed outside internal/exchange in %v: read the rate through exchange.DivineRateReader (POE-308)", found)
	}
}

func calleeName(fun ast.Expr) string {
	switch f := fun.(type) {
	case *ast.Ident:
		return f.Name
	case *ast.SelectorExpr:
		return f.Sel.Name
	}
	return ""
}

func selectorName(e ast.Expr) string {
	if s, ok := e.(*ast.SelectorExpr); ok {
		return s.Sel.Name
	}
	return ""
}
