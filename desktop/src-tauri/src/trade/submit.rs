use crate::ssot::{note_server_league, parse_league_from_status, LeagueResolver};

/// Submit the captured row, including terminal dispatch and reconciliation.
/// Only overload responses retry; mismatches schedule resolution once and end.
pub(crate) async fn submit_trade_result(
    http: &reqwest::Client,
    url: &str,
    result: &crate::trade::types::TradeLookupResult,
    resolver: impl LeagueResolver,
    log: impl Fn(String),
) {
    for attempt in 1..=crate::TRADE_SUBMIT_ATTEMPTS {
        match http.post(url).json(result).send().await {
            Ok(res) if res.status().is_success() => {
                log(format!("Trade submitted to server: {} ({})", result.gem, result.variant));
                return;
            }
            Ok(res) if res.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE
                && attempt < crate::TRADE_SUBMIT_ATTEMPTS =>
            {
                let delay = crate::retry_after_delay(
                    res.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()),
                );
                log(format!(
                    "Trade submit shed by server (503), retrying in {:.1}s (attempt {}/{})",
                    delay.as_secs_f64(), attempt, crate::TRADE_SUBMIT_ATTEMPTS,
                ));
                tokio::time::sleep(delay).await;
            }
            Ok(res) if res.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE => {
                log(format!(
                    "Trade submit shed by server after {} attempts — history row lost: {} ({})",
                    crate::TRADE_SUBMIT_ATTEMPTS, result.gem, result.variant,
                ));
                return;
            }
            Ok(res) if res.status() == reqwest::StatusCode::CONFLICT => {
                let observed = reconcile_trade_mismatch(res, |observed| {
                    note_server_league(resolver, observed);
                }).await;
                log(format!(
                    "Trade submit rejected: league mismatch; server league: {}; resolving server league",
                    observed.as_deref().unwrap_or("unknown"),
                ));
                return;
            }
            Ok(res) => {
                let status = res.status();
                let body = res.text().await.unwrap_or_default();
                log(format!("Trade submit rejected: {} — {}", status, crate::body_excerpt(&body, 200)));
                return;
            }
            Err(e) => {
                log(format!("Trade submit failed: {}", e));
                return;
            }
        }
    }
}

/// Consume a 409 once. A malformed hint still asks for authoritative status.
pub(crate) async fn reconcile_trade_mismatch(
    response: reqwest::Response,
    reconcile: impl FnOnce(Option<&str>),
) -> Option<String> {
    let observed = response.json::<serde_json::Value>().await.ok()
        .and_then(|body| parse_league_from_status(&body));
    reconcile(observed.as_deref());
    observed
}

#[cfg(test)]
mod trade_submit_tests {
    use super::*;
    use std::time::Duration;
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncWriteExt;

    struct RecordingResolver<'a> {
        current: &'a Mutex<Option<String>>,
        scheduled: &'a Mutex<usize>,
    }

    impl LeagueResolver for RecordingResolver<'_> {
        fn current_league(&self) -> Option<String> {
            self.current.lock().unwrap().clone()
        }

        fn rearm(self) {
            *self.scheduled.lock().unwrap() += 1;
        }
    }

    async fn submit_responses(responses: &[(u16, &str)]) -> (Vec<String>, usize, Vec<String>) {
        submit_responses_in_league(responses, "League A").await
    }

    async fn submit_responses_in_league(responses: &[(u16, &str)], current_name: &str) -> (Vec<String>, usize, Vec<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/api/trade/submit", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let responses: Vec<_> = responses.iter().map(|(status, body)| (*status, body.to_string())).collect();
        let server = tokio::spawn(async move {
            for (status, body) in responses {
                let Ok(accepted) = tokio::time::timeout(Duration::from_secs(1), listener.accept()).await else {
                    break;
                };
                let (mut socket, _) = accepted.unwrap();
                let request = crate::trade::client::tests::read_request(&mut socket).await;
                seen.lock().unwrap().push(request);
                let response = format!(
                    "HTTP/1.1 {status} Stub\r\ncontent-type: application/json\r\nretry-after: 0\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len(),
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                socket.shutdown().await.unwrap();
            }
            // Keep the endpoint open to observe any forbidden extra submission.
            if let Ok(Ok((mut socket, _))) = tokio::time::timeout(Duration::from_millis(100), listener.accept()).await {
                let request = crate::trade::client::tests::read_request(&mut socket).await;
                seen.lock().unwrap().push(request);
            }
        });
        let current = Mutex::new(Some(current_name.to_string()));
        let scheduled = Mutex::new(0);
        let logs = Mutex::new(Vec::new());
        let result = crate::trade::signals::build_result("Gem", "20/20", "League A", "", 0, vec![], 0.0);
        submit_trade_result(&reqwest::Client::new(), &url, &result,
            RecordingResolver { current: &current, scheduled: &scheduled },
            |line| logs.lock().unwrap().push(line),
        ).await;
        server.await.unwrap();
        let requests = requests.lock().unwrap().clone();
        for request in &requests {
            let body = request.split_once("\r\n\r\n").unwrap().1;
            let json: serde_json::Value = serde_json::from_str(body).unwrap();
            assert_eq!(json["league"], "League A", "never relabel the old contribution");
        }
        assert_eq!(current.into_inner().unwrap().as_deref(), Some(current_name), "a hint never replaces authoritative league state");
        (requests, scheduled.into_inner().unwrap(), logs.into_inner().unwrap())
    }

    #[tokio::test]
    async fn trade_submit_mismatch_reconciles_once_without_resubmission() {
        let (requests, scheduled, logs) = submit_responses(&[(409, r#"{"error":"league_mismatch","league":"League B"}"#)]).await;
        assert_eq!(requests.len(), 1);
        assert_eq!(scheduled, 1, "different league schedules reconciliation exactly once");
        assert_eq!(logs, vec!["Trade submit rejected: league mismatch; server league: League B; resolving server league"]);
    }

    #[tokio::test]
    async fn trade_submit_malformed_mismatch_reconciles_unknown_once() {
        let (requests, scheduled, logs) = submit_responses(&[(409, "not JSON")]).await;
        assert_eq!(requests.len(), 1);
        assert_eq!(scheduled, 1, "unknown league schedules reconciliation exactly once");
        assert_eq!(logs, vec!["Trade submit rejected: league mismatch; server league: unknown; resolving server league"]);
    }

    #[tokio::test]
    async fn trade_submit_matching_hint_does_not_schedule_reconciliation() {
        let (requests, scheduled, logs) = submit_responses_in_league(
            &[(409, r#"{"error":"league_mismatch","league":"League B"}"#)], "League B",
        ).await;
        assert_eq!(requests.len(), 1);
        assert_eq!(scheduled, 0, "the standing league already agrees with the hint");
        assert_eq!(logs, vec!["Trade submit rejected: league mismatch; server league: League B; resolving server league"]);
    }

    #[tokio::test]
    async fn trade_submit_overload_retries_the_captured_row() {
        let (requests, scheduled, logs) = submit_responses(&[(503, "busy"), (503, "busy"), (200, "{}")]).await;
        assert_eq!(requests.len(), 3);
        assert_eq!(scheduled, 0);
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[2], "Trade submitted to server: Gem (20/20)");
    }

    #[tokio::test]
    async fn trade_submit_overload_stops_at_the_retry_bound() {
        let (requests, scheduled, logs) = submit_responses(&[(503, "busy"), (503, "busy"), (503, "busy")]).await;
        assert_eq!(requests.len(), 3);
        assert_eq!(scheduled, 0);
        assert_eq!(logs[2], "Trade submit shed by server after 3 attempts — history row lost: Gem (20/20)");
    }
}
