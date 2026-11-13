package resolve

import "strings"

// Kind names the market family a Key prices.
type Kind string

const (
	KindGem      Kind = "gem"
	KindCurrency Kind = "currency"
)

// Key identifies one priced market in one league.
type Key struct {
	League    string
	Kind      Kind
	Name      string
	Variant   string
	Corrupted bool
}

// NormalizeVariant converts frontend variant format ("1/0", "20/0") to DB format
// ("1", "20"). The DB stores variants without the "/0" suffix for zero-quality gems.
// Variants with quality ("1/20", "20/20") pass through unchanged.
// TODO: Remove this once POE-52 normalizes DB storage to full "level/quality" format.
func NormalizeVariant(v string) string {
	if strings.HasSuffix(v, "/0") {
		return strings.TrimSuffix(v, "/0")
	}
	return v
}

// GemKey builds the market key for a gem variant. A trailing "c" on the
// variant ("21/20c") marks the corrupted market.
func GemKey(league, name, variant string) Key {
	corrupted := strings.HasSuffix(variant, "c")
	return Key{
		League:    league,
		Kind:      KindGem,
		Name:      name,
		Variant:   NormalizeVariant(strings.TrimSuffix(variant, "c")),
		Corrupted: corrupted,
	}
}

// CurrencyKey builds the market key for a currency id ("divine").
func CurrencyKey(league, id string) Key {
	return Key{League: league, Kind: KindCurrency, Name: id}
}

// String renders the key as League|Kind|Name|Variant|c? for logs only.
func (k Key) String() string {
	s := k.League + "|" + string(k.Kind) + "|" + k.Name + "|" + k.Variant
	if k.Corrupted {
		s += "|c"
	}
	return s
}
