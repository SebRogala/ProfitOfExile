package harvest

import (
	"bytes"
	"fmt"
	"os"
	"slices"
	"strings"
	"testing"
)

const desktopFamilyFixture = "../../desktop/src/lib/harvest/__fixtures__/harvest.json"

func TestEmbeddedFamilies_matchDesktopFixture(t *testing.T) {
	fixture, err := os.ReadFile(desktopFamilyFixture)
	if err != nil {
		t.Fatalf("read desktop fixture %s: %v", desktopFamilyFixture, err)
	}
	if !bytes.Equal(fixture, familiesJSON) {
		t.Fatalf("internal/harvest/familydata/families.json differs from %s; "+
			"copy the desktop fixture over the embedded file", desktopFamilyFixture)
	}
}

func TestFamilies_servesSevenFamiliesInFileOrder(t *testing.T) {
	var ids []string
	for _, f := range Families() {
		ids = append(ids, f.ID)
	}
	want := []string{"delirium", "essence", "corrupt", "fossil", "astrolabe", "oil", "catalyst"}
	if !slices.Equal(ids, want) {
		t.Fatalf("family ids = %v, want %v", ids, want)
	}
}

func TestFamilies_fossilHasTheLoggedSample(t *testing.T) {
	fossil := Families()[3]
	if fossil.ID != "fossil" {
		t.Fatalf("Families()[3].ID = %q, want fossil", fossil.ID)
	}
	if fossil.Weights == nil {
		t.Fatal("fossil Weights = nil, want the logged table")
	}
	if fossil.Weights.Sample.Rolls == nil || *fossil.Weights.Sample.Rolls != 10895 {
		t.Fatalf("fossil sample rolls = %v, want 10895", fossil.Weights.Sample.Rolls)
	}
	if got := len(fossil.Weights.Types); got != 25 {
		t.Fatalf("fossil type count = %d, want 25", got)
	}
	first := fossil.Weights.Types[0]
	if first.Name != "Metallic Fossil" {
		t.Fatalf("fossil first type = %q, want Metallic Fossil", first.Name)
	}
	if first.LoggedRolls == nil || *first.LoggedRolls != 1215 {
		t.Fatalf("Metallic Fossil loggedRolls = %v, want 1215", first.LoggedRolls)
	}
}

func TestFamilies_uniformFamilyKeepsNullRolls(t *testing.T) {
	essence := Families()[1]
	if essence.ID != "essence" {
		t.Fatalf("Families()[1].ID = %q, want essence", essence.ID)
	}
	if !essence.Weights.Sample.Uniform {
		t.Fatal("essence sample Uniform = false, want true")
	}
	var greed *WeightType
	for i := range essence.Weights.Types {
		if essence.Weights.Types[i].ItemID == "Metadata/Items/Currency/CurrencyEssenceGreed7" {
			greed = &essence.Weights.Types[i]
		}
	}
	if greed == nil {
		t.Fatal("essence has no Greed7 type")
	}
	if greed.LoggedRolls != nil {
		t.Fatalf("Greed7 loggedRolls = %d, want null for a uniform family", *greed.LoggedRolls)
	}
}

func TestFamilies_returnsACopy(t *testing.T) {
	first := Families()
	first[0].Label = "mutated"
	first[0].Weights.Types[0].Name = "mutated"

	second := Families()
	if second[0].Label == "mutated" {
		t.Fatal("mutating a returned Label changed the package copy")
	}
	if second[0].Weights.Types[0].Name == "mutated" {
		t.Fatal("mutating a returned type name changed the package copy")
	}
}

func TestFamilies_samplePointersAreCopied(t *testing.T) {
	first := Families()
	*first[3].Weights.Sample.Rolls = 1

	second := Families()
	if got := *second[3].Weights.Sample.Rolls; got != 10895 {
		t.Fatalf("fossil sample rolls after caller mutation = %d, want 10895", got)
	}
}

func TestItemIDs_coversEveryWeightedType(t *testing.T) {
	ids := ItemIDs()
	if len(ids) != 93 {
		t.Fatalf("len(ItemIDs()) = %d, want 93 weighted types across all families", len(ids))
	}
	if !slices.Contains(ids, "Metadata/Items/Currency/CurrencyDelveCraftingAbyss") {
		t.Fatal("ItemIDs() does not contain CurrencyDelveCraftingAbyss")
	}
}

func TestFamilies_everyPointerIsCopied(t *testing.T) {
	f := Families()[3]
	if f.ID != "fossil" {
		t.Fatalf("Families()[3].ID = %q, want fossil", f.ID)
	}
	*f.Lifeforce = "x"
	*f.RerollCost = 999
	*f.Note = "x"
	*f.Weights.Sample.LifeforceSpent = 1
	*f.Weights.Sample.Date = "x"
	*f.Weights.Types[0].LoggedRolls = 1
	f.Weights.Types[1] = WeightType{}

	again := Families()[3]
	if again.Lifeforce == nil || *again.Lifeforce != "Wild" {
		t.Fatalf("fossil lifeforce after caller mutation = %s, want Wild", show(again.Lifeforce))
	}
	if again.RerollCost == nil || *again.RerollCost != 30 {
		t.Fatalf("fossil rerollCost after caller mutation = %s, want 30", show(again.RerollCost))
	}
	if again.Note == nil || *again.Note == "x" {
		t.Fatalf("fossil note after caller mutation = %s, want the original note", show(again.Note))
	}
	if got := *again.Weights.Sample.LifeforceSpent; got != 327270 {
		t.Fatalf("fossil lifeforceSpent after caller mutation = %d, want 327270", got)
	}
	if got := *again.Weights.Sample.Date; got != "2026-10-12" {
		t.Fatalf("fossil sample date after caller mutation = %q, want 2026-10-12", got)
	}
	if got := *again.Weights.Types[0].LoggedRolls; got != 1215 {
		t.Fatalf("Metallic Fossil loggedRolls after caller mutation = %d, want 1215", got)
	}
	if got := again.Weights.Types[1].Name; got != "Jagged Fossil" {
		t.Fatalf("fossil type 1 after caller mutation = %q, want Jagged Fossil", got)
	}
}

func TestMustParseFamilies_panicsOnMalformedJSON(t *testing.T) {
	var recovered any
	func() {
		defer func() { recovered = recover() }()
		mustParseFamilies([]byte("{not json"))
	}()
	if recovered == nil {
		t.Fatal("mustParseFamilies on malformed JSON did not panic")
	}
	msg := fmt.Sprint(recovered)
	for _, want := range []string{"harvest", "familydata/families.json"} {
		if !strings.Contains(msg, want) {
			t.Fatalf("panic message %q does not contain %q", msg, want)
		}
	}
}

// show prints a pointer's value, or <nil>, so a failure names the value rather than an address.
func show[T any](p *T) string {
	if p == nil {
		return "<nil>"
	}
	return fmt.Sprint(*p)
}
