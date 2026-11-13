package lab

import (
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

// divineSnapshotLiteral is the poe.ninja divine snapshot filter. Its one home is
// Repository.LatestNinjaDivinePrice, read through exchange.DivineRateReader.
const divineSnapshotLiteral = "currency_id = 'divine'"

func TestDivineSnapshotQuery_livesOnlyInTheLabRepository(t *testing.T) {
	const root = "../.."
	var found []string
	parsedRepository := false
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
			src, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			if rel == "internal/lab/repository.go" {
				parsedRepository = true
			}
			if strings.Contains(string(src), divineSnapshotLiteral) {
				found = append(found, rel)
			}
			return nil
		})
		if err != nil {
			t.Fatalf("walk %s: %v", dir, err)
		}
	}

	if !parsedRepository {
		t.Fatal("the walk never read internal/lab/repository.go, so the guard checked nothing")
	}
	if want := []string{"internal/lab/repository.go"}; !reflect.DeepEqual(found, want) {
		t.Errorf("files holding %q = %v, want only %v: read the rate through exchange.DivineRateReader (POE-339)", divineSnapshotLiteral, found, want)
	}
}
