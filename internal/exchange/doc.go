// Package exchange reads Path of Exile's public currency exchange feed and
// turns one published hour into normalized market rows.
//
// The feed is GET https://web.poecdn.com/api/currency-exchange[/{realm}][/{id}]
// where {id} is a unix hour. Observed 2026-08-19: no auth, no ETag, no Age and
// no rate-limit headers; a published hour answers 200 with roughly 1.7 MB of
// JSON, an unpublished (current or future) hour answers 404 with the body
// {"next_change_id":<current unpublished hour>,"markets":[]}, and an omitted id
// answers 200 with the oldest retained hour.
//
// next_change_id is a cursor, not the hour the body describes: on a 200 it names
// the following hour (hour 1787119200 answered 1787122800), and on a 404 it names
// the hour to retry.
//
// The package is pure: no database, no scheduler, no HTTP server, no collector.
// Callers fetch a payload with Client.FetchHour and derive rows with Normalize;
// Normalize, PriceOf and Ratio are the only places prices are computed.
//
// Naming rule: the Go package is named exchange, and every user-facing string
// (User-Agent, error text, log message) says "currency-exchange". A bare "cx"
// identifier is never used.
package exchange
