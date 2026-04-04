package lab

import (
	"fmt"
	"regexp"
	"strings"
)

// LabLayout is the strict Go representation of a poelab.com daily layout JSON.
type LabLayout struct {
	Difficulty     string    `json:"difficulty"`
	Date           string    `json:"date"`
	Weapon         string    `json:"weapon"`
	Phase1         string    `json:"phase1"`
	Phase2         string    `json:"phase2"`
	Trap1          string    `json:"trap1"`
	Trap2          string    `json:"trap2"`
	Section1Layout string    `json:"section1layout,omitempty"`
	Section2Layout string    `json:"section2layout,omitempty"`
	Section3Layout string    `json:"section3layout,omitempty"`
	Rooms          []LabRoom `json:"rooms"`
}

// LabRoom represents a single room in the lab layout.
type LabRoom struct {
	Name              string            `json:"name"`
	AreaCode          string            `json:"areacode"`
	ID                string            `json:"id"`
	X                 string            `json:"x"`
	Y                 string            `json:"y"`
	Dangerous         string            `json:"dangerous"`
	Contents          []string          `json:"contents"`
	ContentDirections []string          `json:"content_directions"`
	Exits             map[string]string `json:"exits"`
	SecretPassage     string            `json:"secret_passage,omitempty"`
}

var (
	validDifficulties = map[string]bool{
		"Normal": true, "Cruel": true, "Merciless": true, "Uber": true,
	}
	validDirections = map[string]bool{
		"N": true, "NE": true, "E": true, "SE": true,
		"S": true, "SW": true, "W": true, "NW": true,
		"C": true,
	}
	validAreaCodes = map[string]bool{
		"c_branch": true, "c_branch_bottleneck_1": true, "c_branch_bottleneck_2": true,
		"c_branch_door": true, "c_end_bottleneck": true, "c_quad": true,
		"c_quad_door": true, "c_straight": true, "c_straight_bottleneck": true,
		"dg_branch": true, "dg_branch_bottleneck_1": true, "dg_branch_bottleneck_2": true,
		"dg_branch_door": true, "dg_end_bottleneck": true, "dg_quad": true,
		"dg_quad_door": true, "dg_straight": true, "dg_straight_bottleneck": true,
		"eh_block_branch": true, "eh_branch": true, "eh_branch_bottleneck_2": true,
		"eh_branch_door": true, "eh_end_bottleneck": true, "eh_quad": true,
		"eh_quad_door": true, "eh_straight": true, "eh_straight_bottleneck": true,
		"oh_branch": true, "oh_branch_bottleneck_1": true, "oh_branch_bottleneck_2": true,
		"oh_branch_door": true, "oh_end_bottleneck": true, "oh_quad": true,
		"oh_quad_door": true, "oh_straight": true, "oh_straight_bottleneck": true,
		"p_branch": true, "p_branch_bottleneck_1": true, "p_branch_bottleneck_2": true,
		"p_branch_door": true, "p_end_bottleneck": true, "p_quad": true,
		"p_straight": true, "p_straight_bottleneck": true,
		"rt_branch": true, "rt_branch_bottleneck_1": true, "rt_branch_bottleneck_2": true,
		"rt_branch_door": true, "rt_end_bottleneck": true, "rt_quad": true,
		"rt_quad_door": true, "rt_straight": true, "rt_straight_bottleneck": true,
	}

	datePattern     = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}$`)
	roomNamePattern = regexp.MustCompile(`^[A-Za-z' ]+$`)
	roomIDPattern   = regexp.MustCompile(`^[A-Za-z0-9]+$`)
	safeStrPattern  = regexp.MustCompile(`^[A-Za-z0-9_ ]+$`)
	htmlTagPattern  = regexp.MustCompile(`<[^>]*>`)
)

// ValidateLayout checks a parsed LabLayout against business rules.
// Returns a descriptive error on the first validation failure, nil if valid.
func ValidateLayout(layout *LabLayout) error {
	if !validDifficulties[layout.Difficulty] {
		return fmt.Errorf("invalid difficulty %q", layout.Difficulty)
	}
	if !datePattern.MatchString(layout.Date) {
		return fmt.Errorf("invalid date format %q (expected YYYY-MM-DD)", layout.Date)
	}

	for _, field := range []struct {
		name, value string
	}{
		{"weapon", layout.Weapon},
		{"phase1", layout.Phase1},
		{"phase2", layout.Phase2},
		{"trap1", layout.Trap1},
		{"trap2", layout.Trap2},
	} {
		if len(field.value) > 50 {
			return fmt.Errorf("%s too long (%d chars, max 50)", field.name, len(field.value))
		}
		if field.value != "" && !safeStrPattern.MatchString(field.value) {
			return fmt.Errorf("%s contains invalid characters: %q", field.name, field.value)
		}
	}

	if len(layout.Rooms) == 0 {
		return fmt.Errorf("rooms array is empty")
	}
	if len(layout.Rooms) > 50 {
		return fmt.Errorf("too many rooms (%d, max 50)", len(layout.Rooms))
	}

	roomIDs := make(map[string]bool, len(layout.Rooms))
	for _, room := range layout.Rooms {
		roomIDs[room.ID] = true
	}

	for i, room := range layout.Rooms {
		if err := validateRoom(&room, roomIDs, i); err != nil {
			return err
		}
	}

	return nil
}

func validateRoom(room *LabRoom, validIDs map[string]bool, index int) error {
	if room.ID == "" {
		return fmt.Errorf("room[%d]: id is empty", index)
	}
	if len(room.ID) > 10 || !roomIDPattern.MatchString(room.ID) {
		return fmt.Errorf("room[%d]: invalid id %q", index, room.ID)
	}

	if room.Name == "" {
		return fmt.Errorf("room[%d]: name is empty", index)
	}
	if len(room.Name) > 100 || !roomNamePattern.MatchString(room.Name) {
		return fmt.Errorf("room[%d]: invalid name %q", index, room.Name)
	}

	if room.AreaCode != "" && !validAreaCodes[room.AreaCode] {
		return fmt.Errorf("room[%d]: invalid area code %q", index, room.AreaCode)
	}

	for dir, target := range room.Exits {
		if !validDirections[dir] {
			return fmt.Errorf("room[%d]: invalid exit direction %q", index, dir)
		}
		if !validIDs[target] {
			return fmt.Errorf("room[%d]: exit %s references unknown room ID %q", index, dir, target)
		}
	}

	for j, content := range room.Contents {
		if len(content) > 100 {
			return fmt.Errorf("room[%d].contents[%d]: too long (%d chars, max 100)", index, j, len(content))
		}
		if htmlTagPattern.MatchString(content) {
			return fmt.Errorf("room[%d].contents[%d]: HTML tags not allowed", index, j)
		}
	}

	return nil
}

// SanitizeLayout strips any HTML tags from user-controlled string fields.
func SanitizeLayout(layout *LabLayout) {
	layout.Difficulty = strings.TrimSpace(layout.Difficulty)
	layout.Date = strings.TrimSpace(layout.Date)
	for i := range layout.Rooms {
		layout.Rooms[i].Name = strings.TrimSpace(htmlTagPattern.ReplaceAllString(layout.Rooms[i].Name, ""))
		if layout.Rooms[i].Name == "" || layout.Rooms[i].Name == "manually added" {
			layout.Rooms[i].Name = "unknown room"
		}
		areaCode := strings.TrimSpace(htmlTagPattern.ReplaceAllString(layout.Rooms[i].AreaCode, ""))
		if areaCode == "none" {
			areaCode = ""
		}
		layout.Rooms[i].AreaCode = areaCode
		layout.Rooms[i].ID = strings.TrimSpace(layout.Rooms[i].ID)
		for j := range layout.Rooms[i].Contents {
			layout.Rooms[i].Contents[j] = htmlTagPattern.ReplaceAllString(layout.Rooms[i].Contents[j], "")
		}
	}
}
