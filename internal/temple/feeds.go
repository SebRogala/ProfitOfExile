package temple

import (
	_ "embed"
	"encoding/json"
	"fmt"

	"profitofexile/internal/mercure"
)

// THE SEVEN FEEDS THIS PACKAGE READS
//
// POE-254's collector polls seven poe.ninja item-overview categories and stores
// them in item_snapshots; this package is their read side (POE-255). The read
// side's one home is feeds.json, embedded here and also read by
// scripts/generate-temple-icons.py. The same seven appear on both sides, and
// they are listed in feeds.json rather than imported from
// internal/collector on purpose: nothing under internal/server imports the
// collector package today (internal/lab reads gem_snapshots without importing
// the writer that fills it), and the two lists answer different questions —
// which categories to POLL versus which to READ.
//
// Drift between them is the real hazard, so it is closed by a test rather than
// by an import: TestFeeds_matchCollectorItemEndpoints asserts category, endpoint
// name and full topic against collector.ItemEndpoints, one-for-one, in the test
// binary only. A category added to the collector and not here fails that test
// before it can quietly go unread.

// mercureTopicPrefix mirrors internal/collector's namespace for the shared hub.
// It is spelled out in Feed.Topic below rather than concatenated at use sites so
// the parity test compares the exact strings the server subscribes to.
const mercureTopicPrefix = mercure.CollectorTopicPrefix

// RoomCategory is the item_snapshots category carrying the 86 temple room-tier
// lines. It is the only category whose lines are rooms; the other six carry the
// uniques the upgrade table names (and the Vial feed, still read).
const RoomCategory = "IncursionTemple"

// Feed binds one poe.ninja item-overview category to the collector's endpoint
// name (which the Mercure payload carries in its "endpoint" field) and to the
// full topic the server subscribes to.
type Feed struct {
	Category string
	Endpoint string
	Topic    string
}

//go:embed feeds.json
var feedsJSON []byte

// feeds is the seven categories decoded from feeds.json, room category first.
var feeds = decodeFeeds(feedsJSON)

// feedEntry is one row of feeds.json.
type feedEntry struct {
	Category    string `json:"category"`
	Endpoint    string `json:"endpoint"`
	TopicSuffix string `json:"topicSuffix"`
}

// decodeFeeds panics on a malformed, empty or blank-field table, or one whose
// first entry is not RoomCategory: a server that would otherwise read no temple
// feed must not start silently.
func decodeFeeds(raw []byte) []Feed {
	var entries []feedEntry
	if err := json.Unmarshal(raw, &entries); err != nil {
		panic(fmt.Sprintf("temple: decode feeds.json: %v", err))
	}
	if len(entries) == 0 {
		panic("temple: feeds.json holds no feeds")
	}
	if entries[0].Category != RoomCategory {
		panic(fmt.Sprintf("temple: feeds.json first category is %q, want %q", entries[0].Category, RoomCategory))
	}
	out := make([]Feed, len(entries))
	for i, e := range entries {
		if e.Category == "" || e.Endpoint == "" || e.TopicSuffix == "" {
			panic(fmt.Sprintf("temple: feeds.json feed %d has an empty field", i))
		}
		out[i] = Feed{Category: e.Category, Endpoint: e.Endpoint, Topic: mercureTopicPrefix + e.TopicSuffix}
	}
	return out
}

// Categories returns the seven item_snapshots categories this package reads, in
// feeds order. The returned slice is fresh; it is passed straight into a query
// parameter and must not alias package state.
func Categories() []string {
	out := make([]string, len(feeds))
	for i, f := range feeds {
		out[i] = f.Category
	}
	return out
}

// Topics returns the seven Mercure topics the server subscribes to so that a
// stored item tick recomputes the temple market.
func Topics() []string {
	out := make([]string, len(feeds))
	for i, f := range feeds {
		out[i] = f.Topic
	}
	return out
}

// IsFeedEndpoint reports whether a collector endpoint name (the Mercure
// payload's "endpoint" field) belongs to one of the seven feeds.
//
// The server dispatches on the endpoint name rather than the topic because the
// generic collector branch in cmd/server has already parsed the payload and
// validated the league stamp by the time it decides what to run.
func IsFeedEndpoint(endpoint string) bool {
	for _, f := range feeds {
		if f.Endpoint == endpoint {
			return true
		}
	}
	return false
}
