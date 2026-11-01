// Command boss-loot-import imports the fixed POE-317 CatarinaRuns seed source.
package main

import (
	"archive/zip"
	"context"
	"encoding/json"
	"encoding/xml"
	"flag"
	"fmt"
	"io"
	"os"
	"path"
	"reflect"
	"strconv"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"profitofexile/internal/bossloot"
	"profitofexile/internal/db"
	"profitofexile/internal/league"
)

type seedRow struct {
	Run       int            `json:"run"`
	SourceRow int            `json:"sourceRow"`
	Date      string         `json:"date"`
	Items     bossloot.Items `json:"items"`
}

// These are source-column mappings, not a server-owned loot profile. Keep the
// catalogue keys consistent with the WI-1 boundary adapter and Rust profile.
var retainedKeys = []string{
	"Metadata/Items/Currency/CurrencyRerollRareVeiled",
	"Metadata/Items/MapFragments/AbyssPinnacleKeyGhastly",
	"Metadata/Items/MapFragments/BetrayalAllflamePack",
	"Cinderswallow Urn", "Cane of Kulemak", "Spinehail", "The Queen's Hunger", "Bitterbind Point", "The Devouring Diadem",
}

var sourceHeaders = []string{"Run #", "Veiled Exalted", "Eye", "Allflame", "Flask", "Staff", "Quiver", "Body armour", "Shield", "Helmet", "Support", "Templar Astrolabe"}
var sourceProfile = bossloot.Profile{Boss: "catarina", ProfileKey: "Normal", ProfileVersion: 1}

type richString struct {
	Text string `xml:"t"`
	Runs []struct {
		Text string `xml:"t"`
	} `xml:"r"`
}

func (s richString) value() string {
	var b strings.Builder
	b.WriteString(s.Text)
	for _, r := range s.Runs {
		b.WriteString(r.Text)
	}
	return b.String()
}

type cell struct {
	Ref    string     `xml:"r,attr"`
	Type   string     `xml:"t,attr"`
	Value  string     `xml:"v"`
	Inline richString `xml:"is"`
}

func (c cell) text(shared []richString) (string, error) {
	switch c.Type {
	case "s":
		i, err := strconv.Atoi(c.Value)
		if err != nil || i < 0 || i >= len(shared) {
			return "", fmt.Errorf("invalid shared string at %s", c.Ref)
		}
		return shared[i].value(), nil
	case "inlineStr":
		return c.Inline.value(), nil
	case "", "n":
		return c.Value, nil // A formula's cached value, never its expression.
	default:
		return "", fmt.Errorf("unsupported cell type at %s", c.Ref)
	}
}

func readWorkbook(r io.ReaderAt, size int64) ([]seedRow, error) {
	z, err := zip.NewReader(r, size)
	if err != nil {
		return nil, fmt.Errorf("read XLSX archive: %w", err)
	}
	files := map[string]*zip.File{}
	for _, f := range z.File {
		files[f.Name] = f
	}
	decode := func(name string, out any) error {
		f, ok := files[name]
		if !ok {
			return fmt.Errorf("missing XLSX part %s", name)
		}
		in, err := f.Open()
		if err != nil {
			return err
		}
		defer in.Close()
		return xml.NewDecoder(in).Decode(out)
	}
	var shared struct {
		Strings []richString `xml:"si"`
	}
	if _, ok := files["xl/sharedStrings.xml"]; ok {
		if err := decode("xl/sharedStrings.xml", &shared); err != nil {
			return nil, err
		}
	}
	var tablePath string
	for _, f := range z.File {
		if !strings.HasPrefix(f.Name, "xl/tables/") || !strings.HasSuffix(f.Name, ".xml") {
			continue
		}
		var table struct {
			Name    string `xml:"name,attr"`
			Ref     string `xml:"ref,attr"`
			Columns []struct {
				Name string `xml:"name,attr"`
			} `xml:"tableColumns>tableColumn"`
		}
		if err := decode(f.Name, &table); err != nil {
			return nil, err
		}
		if table.Name != "CatarinaRuns" {
			continue
		}
		if tablePath != "" {
			return nil, fmt.Errorf("duplicate CatarinaRuns table")
		}
		if table.Ref != "A3:L81" || len(table.Columns) != len(sourceHeaders) {
			return nil, fmt.Errorf("CatarinaRuns must span A3:L81 with twelve columns")
		}
		for i, col := range table.Columns {
			if col.Name != sourceHeaders[i] {
				return nil, fmt.Errorf("unexpected CatarinaRuns column %d", i+1)
			}
		}
		tablePath = f.Name
	}
	if tablePath == "" {
		return nil, fmt.Errorf("CatarinaRuns table missing")
	}
	var cells map[string]cell
	for _, f := range z.File {
		if path.Dir(f.Name) != "xl/worksheets" || !strings.HasSuffix(f.Name, ".xml") {
			continue
		}
		var sheet struct {
			Rows []struct {
				Cells []cell `xml:"c"`
			} `xml:"sheetData>row"`
			Tables []struct {
				ID string `xml:"id,attr"`
			} `xml:"tableParts>tablePart"`
		}
		if err := decode(f.Name, &sheet); err != nil {
			return nil, err
		}
		if len(sheet.Tables) == 0 {
			continue
		}
		var rels struct {
			Items []struct {
				ID     string `xml:"Id,attr"`
				Target string `xml:"Target,attr"`
				Mode   string `xml:"TargetMode,attr"`
			} `xml:"Relationship"`
		}
		if err := decode(path.Join(path.Dir(f.Name), "_rels", path.Base(f.Name)+".rels"), &rels); err != nil {
			return nil, err
		}
		matches := false
		for _, part := range sheet.Tables {
			for _, rel := range rels.Items {
				target := path.Clean(path.Join(path.Dir(f.Name), rel.Target))
				if strings.HasPrefix(rel.Target, "/") {
					target = strings.TrimPrefix(rel.Target, "/")
				}
				if rel.ID == part.ID && rel.Mode != "External" && target == tablePath {
					matches = true
				}
			}
		}
		if !matches {
			continue
		}
		if cells != nil {
			return nil, fmt.Errorf("CatarinaRuns linked to multiple worksheets")
		}
		cells = map[string]cell{}
		for _, row := range sheet.Rows {
			for _, c := range row.Cells {
				if _, exists := cells[c.Ref]; exists {
					return nil, fmt.Errorf("duplicate cell %s", c.Ref)
				}
				cells[c.Ref] = c
			}
		}
	}
	if cells == nil {
		return nil, fmt.Errorf("CatarinaRuns worksheet relationship missing")
	}
	for i, want := range sourceHeaders {
		value, err := cells[fmt.Sprintf("%c3", 'A'+i)].text(shared.Strings)
		if err != nil {
			return nil, err
		}
		if value != want {
			return nil, fmt.Errorf("unexpected worksheet header %d", i+1)
		}
	}
	rows := make([]seedRow, 0, 78)
	for run := 1; run <= 78; run++ {
		sourceRow := run + 3
		value, err := cells[fmt.Sprintf("A%d", sourceRow)].text(shared.Strings)
		if err != nil {
			return nil, err
		}
		number, err := strconv.Atoi(value)
		if err != nil || number != run {
			return nil, fmt.Errorf("source row %d: expected run %d", sourceRow, run)
		}
		row := seedRow{Run: run, SourceRow: sourceRow, Date: sourceDate(run), Items: bossloot.Items{Quantities: map[string]int64{}, Counted: append([]string{}, retainedKeys...)}}
		// K and L are intentionally unread: neither positives nor known absences.
		for i, key := range retainedKeys {
			value, err := cells[fmt.Sprintf("%c%d", 'B'+i, sourceRow)].text(shared.Strings)
			if err != nil {
				return nil, err
			}
			switch value {
			case "", "0":
			case "1":
				row.Items.Quantities[key] = 1
			default:
				return nil, fmt.Errorf("source row %d: retained columns must be blank, 0 or 1", sourceRow)
			}
		}
		if err := validateRow(row, run-1); err != nil {
			return nil, err
		}
		rows = append(rows, row)
	}
	return rows, nil
}

func sourceDate(run int) string {
	if run <= 63 {
		return "2026-10-27"
	}
	return "2026-10-29"
}

func validateRow(row seedRow, index int) error {
	if row.Run != index+1 || row.SourceRow != index+4 || row.Date != sourceDate(row.Run) {
		return fmt.Errorf("source row %d: invalid run/date provenance", row.SourceRow)
	}
	if !reflect.DeepEqual(row.Items.Counted, retainedKeys) {
		return fmt.Errorf("source row %d: counted set must contain only retained columns", row.SourceRow)
	}
	if err := row.Items.Validate(); err != nil {
		return fmt.Errorf("source row %d: %w", row.SourceRow, err)
	}
	pool := 0
	for i, key := range retainedKeys {
		q, exists := row.Items.Quantities[key]
		if exists && q != 1 {
			return fmt.Errorf("source row %d: expected binary marks", row.SourceRow)
		}
		if i >= 3 && q == 1 {
			pool++
		}
	}
	if pool != 1 {
		return fmt.Errorf("source row %d: expected exactly one pool mark, got %d", row.SourceRow, pool)
	}
	return nil
}

func importRuns(ctx context.Context, pool *pgxpool.Pool, scope league.Scope, device string, rows []seedRow) error {
	if err := scope.Validate(); err != nil {
		return err
	}
	if strings.TrimSpace(device) == "" || strings.TrimSpace(device) != device {
		return fmt.Errorf("explicit device required")
	}
	if len(rows) != 78 {
		return fmt.Errorf("expected 78 source rows")
	}
	for i, row := range rows {
		if err := validateRow(row, i); err != nil {
			return err
		}
	}
	tx, err := pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer tx.Rollback(context.Background())
	// A table lock conflicts with normal INSERT/UPDATE/DELETE writers too. The
	// one-off importer needs no copy of Repository.lockWrites' private key format.
	if _, err = tx.Exec(ctx, `LOCK TABLE boss_loot_runs IN SHARE ROW EXCLUSIVE MODE`); err != nil {
		return err
	}
	var exists bool
	if err = tx.QueryRow(ctx, `SELECT EXISTS(SELECT 1 FROM boss_loot_runs WHERE device_id=$1 AND boss=$2 AND profile_key=$3 AND profile_version=$4)`, device, sourceProfile.Boss, sourceProfile.ProfileKey, sourceProfile.ProfileVersion).Scan(&exists); err != nil {
		return err
	}
	if exists {
		return fmt.Errorf("device/profile already has runs; import refused (including reruns)")
	}
	for _, row := range rows {
		payload := struct {
			bossloot.Items
			Source struct {
				Table         string `json:"table"`
				Row           int    `json:"row"`
				Date          string `json:"date"`
				DatePrecision string `json:"datePrecision"`
			} `json:"source"`
		}{Items: row.Items}
		payload.Source.Table = "CatarinaRuns"
		payload.Source.Row = row.SourceRow
		payload.Source.Date = row.Date
		payload.Source.DatePrecision = "calendar"
		raw, err := json.Marshal(payload)
		if err != nil {
			return err
		}
		// UTC midnight encodes a calendar label for the existing timestamp column;
		// source.datePrecision records that no observed time or timezone is known.
		date, err := time.Parse("2006-01-02", row.Date)
		if err != nil {
			return err
		}
		if _, err = tx.Exec(ctx, `INSERT INTO boss_loot_runs (league,device_id,boss,profile_key,profile_version,run_number,date,seed,items) VALUES ($1,$2,$3,$4,$5,$6,$7,NULL,$8)`, scope.ID(), device, sourceProfile.Boss, sourceProfile.ProfileKey, sourceProfile.ProfileVersion, row.Run, date, raw); err != nil {
			return fmt.Errorf("insert source row %d: %w", row.SourceRow, err)
		}
	}
	return tx.Commit(ctx)
}

func runCLI(args []string, out io.Writer) error {
	flags := flag.NewFlagSet("boss-loot-import", flag.ContinueOnError)
	flags.SetOutput(out)
	workbook := flags.String("workbook", "", "XLSX source (read-only)")
	leagueID := flags.String("league", "", "explicit historical source league")
	device := flags.String("device", "", "explicit existing device fingerprint")
	apply := flags.Bool("apply", false, "write all 78 rows atomically; default prints anonymous preview without DB access")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if flags.NArg() != 0 || *workbook == "" || strings.TrimSpace(*leagueID) == "" || strings.TrimSpace(*leagueID) != *leagueID || strings.TrimSpace(*device) == "" || strings.TrimSpace(*device) != *device {
		return fmt.Errorf("workbook, league and device are required explicit inputs")
	}
	f, err := os.Open(*workbook)
	if err != nil {
		return fmt.Errorf("cannot open source workbook")
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return fmt.Errorf("cannot stat source workbook")
	}
	rows, err := readWorkbook(f, info.Size())
	if err != nil {
		return err
	}
	if !*apply {
		enc := json.NewEncoder(out)
		enc.SetIndent("", "  ")
		return enc.Encode(rows)
	}
	url := os.Getenv("DATABASE_URL")
	if url == "" {
		return fmt.Errorf("DATABASE_URL required for apply")
	}
	ctx := context.Background()
	pool, err := db.NewPool(ctx, url)
	if err != nil {
		return fmt.Errorf("database connection failed")
	}
	defer pool.Close()
	if err = importRuns(ctx, pool, league.Historical(*leagueID), *device, rows); err != nil {
		return err
	}
	_, err = fmt.Fprintln(out, "Imported 78 Catarina runs.")
	return err
}

func main() {
	if err := runCLI(os.Args[1:], os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
