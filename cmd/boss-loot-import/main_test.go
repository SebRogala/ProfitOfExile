package main

import (
	"archive/zip"
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"reflect"
	"strings"
	"testing"
)

var expectedKeys = []string{
	"Metadata/Items/Currency/CurrencyRerollRareVeiled",
	"Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly",
	"Metadata/Items/MapFragments/BetrayalAllflamePack",
	"Cinderswallow Urn", "Cane of Kulemak", "Spinehail", "The Queen's Hunger", "Bitterbind Point", "The Devouring Diadem",
}

// syntheticWorkbook exercises relationship resolution, shared/rich/inline
// strings, sparse cells and cached formula values without private workbook bytes.
func syntheticWorkbook(t *testing.T, alter func(map[string]string)) []byte {
	t.Helper()
	cells := map[string]string{}
	headers := []string{"Run #", "Veiled Exalted", "Eye", "Allflame", "Flask", "Staff", "Quiver", "Body armour", "Shield", "Helmet", "Support", "Templar Astrolabe"}
	var columns strings.Builder
	for i, h := range headers {
		fmt.Fprintf(&columns, `<tableColumn id="%d" name="%s"/>`, i+1, h)
		cells[fmt.Sprintf("%c3", 'A'+i)] = fmt.Sprintf(`<is><t>%s</t></is>`, h)
	}
	for row := 4; row <= 81; row++ {
		cells[fmt.Sprintf("A%d", row)] = fmt.Sprintf(`<f>ROW()-3</f><v>%d</v>`, row-3)
		cells[fmt.Sprintf("E%d", row)] = `<v>1</v>`
	}
	// All nine mappings appear, including each pool unique in isolation.
	for row := 4; row <= 9; row++ {
		delete(cells, fmt.Sprintf("E%d", row))
		cells[fmt.Sprintf("%c%d", 'E'+row-4, row)] = `<v>1</v>`
	}
	cells["B4"], cells["C4"], cells["D4"] = `<v>1</v>`, `<v>1</v>`, `<v>1</v>`
	cells["K4"], cells["L4"] = `<v>999</v>`, `<v>untrusted ignored cell</v>`
	if alter != nil {
		alter(cells)
	}
	var sheet strings.Builder
	sheet.WriteString(`<worksheet xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetData>`)
	for row := 3; row <= 81; row++ {
		fmt.Fprintf(&sheet, `<row r="%d">`, row)
		for col := 'A'; col <= 'L'; col++ {
			ref := fmt.Sprintf("%c%d", col, row)
			if v, ok := cells[ref]; ok {
				typ := "n"
				if strings.Contains(v, "<is>") {
					typ = "inlineStr"
				}
				if ref == "B3" {
					typ, v = "s", `<v>0</v>`
				}
				fmt.Fprintf(&sheet, `<c r="%s" t="%s">%s</c>`, ref, typ, v)
			}
		}
		sheet.WriteString(`</row>`)
	}
	sheet.WriteString(`</sheetData><tableParts><tablePart r:id="lootTable"/></tableParts></worksheet>`)
	files := map[string]string{
		"xl/tables/loot.xml":                  `<table name="CatarinaRuns" ref="A3:L81"><tableColumns>` + columns.String() + `</tableColumns></table>`,
		"xl/worksheets/second.xml":            sheet.String(),
		"xl/worksheets/_rels/second.xml.rels": `<Relationships><Relationship Id="lootTable" Target="../tables/loot.xml" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table"/></Relationships>`,
		"xl/worksheets/first.xml":             `<worksheet><sheetData/></worksheet>`,
		"xl/sharedStrings.xml":                `<sst><si><r><t>Veiled </t></r><r><t>Exalted</t></r></si></sst>`,
	}
	var out bytes.Buffer
	z := zip.NewWriter(&out)
	for name, body := range files {
		w, err := z.Create(name)
		if err != nil {
			t.Fatal(err)
		}
		if _, err = w.Write([]byte(body)); err != nil {
			t.Fatal(err)
		}
	}
	if err := z.Close(); err != nil {
		t.Fatal(err)
	}
	return out.Bytes()
}

func workbookRows(t *testing.T, alter func(map[string]string)) []seedRow {
	t.Helper()
	raw := syntheticWorkbook(t, alter)
	rows, err := readWorkbook(bytes.NewReader(raw), int64(len(raw)))
	if err != nil {
		t.Fatal(err)
	}
	return rows
}

func fixtureRows(t *testing.T) []seedRow {
	t.Helper()
	raw, err := os.ReadFile("testdata/catarina-runs.json")
	if err != nil {
		t.Fatal(err)
	}
	var rows []seedRow
	if err = json.Unmarshal(raw, &rows); err != nil {
		t.Fatal(err)
	}
	return rows
}

func TestWorkbookMapsRetainedColumns(t *testing.T) {
	rows := workbookRows(t, nil)
	want := map[string]int64{expectedKeys[0]: 1, expectedKeys[1]: 1, expectedKeys[2]: 1, expectedKeys[3]: 1}
	if !reflect.DeepEqual(rows[0].Items.Quantities, want) {
		t.Fatalf("first run quantities = %v; want %v", rows[0].Items.Quantities, want)
	}
	for i := 1; i < 6; i++ {
		want := map[string]int64{expectedKeys[i+3]: 1}
		if !reflect.DeepEqual(rows[i].Items.Quantities, want) {
			t.Fatalf("pool run %d = %v; want %v", i+1, rows[i].Items.Quantities, want)
		}
	}
}

func TestWorkbookIgnoredColumnsAreNotEligible(t *testing.T) {
	for _, row := range workbookRows(t, nil) {
		if !reflect.DeepEqual(row.Items.Counted, expectedKeys) {
			t.Fatalf("row %d counted = %v; want nine retained columns only", row.SourceRow, row.Items.Counted)
		}
	}
}

func TestWorkbookDatesUseTicketSplit(t *testing.T) {
	rows := workbookRows(t, nil)
	if len(rows) != 78 {
		t.Fatalf("rows = %d; want 78", len(rows))
	}
	for i, row := range rows {
		want := "2026-10-27"
		if i >= 63 {
			want = "2026-10-29"
		}
		if row.Run != i+1 || row.SourceRow != i+4 || row.Date != want {
			t.Fatalf("run %d provenance = %+v; want row %d, date %s", i+1, row, i+4, want)
		}
	}
}

func TestWorkbookRejectsInvalidPool(t *testing.T) {
	for _, tc := range []struct {
		name  string
		alter func(map[string]string)
	}{
		{"zero", func(c map[string]string) { delete(c, "E4") }},
		{"two", func(c map[string]string) { c["F4"] = `<v>1</v>` }},
		{"quantity two", func(c map[string]string) { c["E4"] = `<v>2</v>` }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			raw := syntheticWorkbook(t, tc.alter)
			_, err := readWorkbook(bytes.NewReader(raw), int64(len(raw)))
			if err == nil || !strings.Contains(err.Error(), "row 4") {
				t.Fatalf("invalid pool error = %v; want source row 4", err)
			}
		})
	}
}

func TestWorkbookRejectsMissingRun(t *testing.T) {
	raw := syntheticWorkbook(t, func(c map[string]string) { delete(c, "A66") })
	_, err := readWorkbook(bytes.NewReader(raw), int64(len(raw)))
	if err == nil || !strings.Contains(err.Error(), "row 66") {
		t.Fatalf("missing run error = %v", err)
	}
}

func TestAnonymousFixtureMatchesSeedTotals(t *testing.T) {
	rows := fixtureRows(t)
	if len(rows) != 78 {
		t.Fatalf("rows=%d; want 78", len(rows))
	}
	totals := map[string]int64{}
	for i, row := range rows {
		if err := validateRow(row, i); err != nil {
			t.Fatal(err)
		}
		for key, q := range row.Items.Quantities {
			totals[key] += q
		}
	}
	want := []int64{26, 33, 22, 24, 14, 16, 7, 6, 11}
	for i, key := range expectedKeys {
		if totals[key] != want[i] {
			t.Errorf("%s=%d; want %d", key, totals[key], want[i])
		}
	}
}

func TestCLIRequiresExplicitInputs(t *testing.T) {
	for _, args := range [][]string{nil, {"-workbook", "source.xlsx", "-device", "synthetic"}, {"-workbook", "source.xlsx", "-league", "historical"}, {"-league", "historical", "-device", "synthetic"}} {
		if err := runCLI(args, &bytes.Buffer{}); err == nil || !strings.Contains(err.Error(), "required") {
			t.Fatalf("args %v: %v; want required-input error", args, err)
		}
	}
}
