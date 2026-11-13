package resolve

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
