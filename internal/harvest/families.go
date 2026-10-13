package harvest

import (
	_ "embed"
	"encoding/json"
	"fmt"
)

// familiesJSON is the normative server copy of the Harvest family data. It is
// byte-identical to the desktop fixture; families_test.go enforces that.
//
//go:embed familydata/families.json
var familiesJSON []byte

// families is parsed once, at package initialisation, for the same reason
// internal/exchange parses its embedded asset at init: a malformed committed
// file is a defect identical in every process, so it should crash at start.
var families = mustParseFamilies(familiesJSON)

// WeightSample is the logged roll sample a family's weights were measured from.
type WeightSample struct {
	Rolls          *int    `json:"rolls"`
	LifeforceSpent *int    `json:"lifeforceSpent"`
	Date           *string `json:"date"`
	Source         string  `json:"source"`
	Uniform        bool    `json:"uniform,omitempty"`
}

// WeightType is one weighted outcome of a family.
type WeightType struct {
	ItemID      string `json:"itemId"`
	Name        string `json:"name"`
	ShortName   string `json:"shortName"`
	LoggedRolls *int   `json:"loggedRolls"`
}

// Weights is a family's weighted outcome table and the sample it came from.
type Weights struct {
	Sample WeightSample `json:"sample"`
	Types  []WeightType `json:"types"`
}

// Family is one Harvest reroll family.
type Family struct {
	ID         string   `json:"id"`
	Label      string   `json:"label"`
	Lifeforce  *string  `json:"lifeforce"`
	RerollCost *int     `json:"rerollCost"`
	Weights    *Weights `json:"weights"`
	Note       *string  `json:"_note"`
}

type familiesFile struct {
	Families []Family `json:"families"`
}

func mustParseFamilies(raw []byte) []Family {
	var file familiesFile
	if err := json.Unmarshal(raw, &file); err != nil {
		panic(fmt.Sprintf("harvest: parse embedded familydata/families.json: %v", err))
	}
	return file.Families
}

// Families returns a deep copy of the family data in file order. Every slice
// and every pointed-to value is copied, so a caller cannot change the package
// copy by mutating the result.
func Families() []Family {
	out := make([]Family, len(families))
	for i, f := range families {
		out[i] = Family{
			ID:         f.ID,
			Label:      f.Label,
			Lifeforce:  copyString(f.Lifeforce),
			RerollCost: copyInt(f.RerollCost),
			Weights:    copyWeights(f.Weights),
			Note:       copyString(f.Note),
		}
	}
	return out
}

// ItemIDs returns the itemId of every weighted type across all families, in
// file order, without duplicates.
func ItemIDs() []string {
	seen := make(map[string]struct{})
	var ids []string
	for _, f := range families {
		if f.Weights == nil {
			continue
		}
		for _, t := range f.Weights.Types {
			if _, ok := seen[t.ItemID]; ok {
				continue
			}
			seen[t.ItemID] = struct{}{}
			ids = append(ids, t.ItemID)
		}
	}
	return ids
}

func copyWeights(w *Weights) *Weights {
	if w == nil {
		return nil
	}
	types := make([]WeightType, len(w.Types))
	for i, t := range w.Types {
		types[i] = WeightType{
			ItemID:      t.ItemID,
			Name:        t.Name,
			ShortName:   t.ShortName,
			LoggedRolls: copyInt(t.LoggedRolls),
		}
	}
	return &Weights{
		Sample: WeightSample{
			Rolls:          copyInt(w.Sample.Rolls),
			LifeforceSpent: copyInt(w.Sample.LifeforceSpent),
			Date:           copyString(w.Sample.Date),
			Source:         w.Sample.Source,
			Uniform:        w.Sample.Uniform,
		},
		Types: types,
	}
}

func copyInt(p *int) *int {
	if p == nil {
		return nil
	}
	v := *p
	return &v
}

func copyString(p *string) *string {
	if p == nil {
		return nil
	}
	v := *p
	return &v
}
