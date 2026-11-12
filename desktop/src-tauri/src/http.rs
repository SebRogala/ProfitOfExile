//! The one place this crate builds an HTTP client (POE-360).
//!
//! Every `reqwest::Client` in production comes from [`client_builder`], so the
//! user's proxy setting reaches every request without per-call wiring. The
//! source-scan test at the bottom fails when a client is built anywhere else.
//! A Blocked config yields clients whose every request fails before a
//! connection opens.

/// Hosts that never go through the proxy: the local dev server
/// (`*.localhost`) and loopback. Two spellings of the same set, one per
/// transport's syntax.
const REQWEST_NO_PROXY: &str = "localhost,127.0.0.1,::1,.localhost";
const WEBVIEW2_BYPASS_LIST: &str = "localhost;127.0.0.1;[::1];*.localhost";

/// WebView2 flags for a blocked proxy: a loopback port nothing listens on
/// (refused at once) plus a resolver that finds no host, local hosts kept.
const BLOCKED_PROXY_SERVER: &str = "http://127.0.0.1:1";
const BLOCKED_HOST_RESOLVER_RULES: &str = "MAP * ~NOTFOUND , EXCLUDE localhost , EXCLUDE *.localhost";

const ACCEPTED_SCHEMES: [&str; 3] = ["http", "https", "socks5"];

/// A proxy URL that every transport accepts: `http://`, `https://` or
/// `socks5://`, without credentials — WebView2's `--proxy-server` cannot carry
/// them, so a `user:pass@` URL would route only part of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyUrl(String);

impl ProxyUrl {
    /// `Ok(None)` for an empty value (no proxy: today's behaviour);
    /// `Err` with a message the Settings page shows as-is.
    pub fn parse(raw: &str) -> Result<Option<ProxyUrl>, String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let url = reqwest::Url::parse(trimmed)
            .map_err(|_| "Not a proxy URL — use socks5://host:port or http://host:port".to_string())?;
        if !ACCEPTED_SCHEMES.contains(&url.scheme()) {
            return Err("Proxy must start with http://, https:// or socks5://".to_string());
        }
        if url.host_str().is_none_or(str::is_empty) {
            return Err("Proxy URL has no host".to_string());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("Proxy credentials are not supported: the app window (WebView2) cannot use them. Use socks5://host:port or http://host:port.".to_string());
        }
        reqwest::Proxy::all(trimmed).map_err(|e| format!("Proxy URL rejected: {e}"))?;
        Ok(Some(ProxyUrl(trimmed.to_string())))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The URL for reqwest clients and the updater: `socks5://` becomes
    /// `socks5h://` so DNS resolves at the proxy, as it does in WebView2.
    pub fn for_http_clients(&self) -> String {
        let mut url = reqwest::Url::parse(&self.0).expect("validated by ProxyUrl::parse");
        if url.scheme() == "socks5" {
            url.set_scheme("socks5h").expect("socks5 and socks5h are both non-special schemes");
            return url.to_string();
        }
        self.0.clone()
    }

    /// `scheme://host[:port]`: Chromium's `--proxy-server` takes no path.
    fn server_for_webview2(&self) -> String {
        let url = reqwest::Url::parse(&self.0).expect("validated by ProxyUrl::parse");
        let host = url.host_str().expect("validated by ProxyUrl::parse");
        match url.port_or_known_default() {
            Some(port) => format!("{}://{}:{}", url.scheme(), host, port),
            None => format!("{}://{}", url.scheme(), host),
        }
    }
}

/// Which transport a client serves: the trade client may opt out of the proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// pathofexile.com trade API (`TradeApiClient`).
    Trade,
    /// Every other client: server, updater.
    Other,
}

/// The end of every blocked reason: what the user does about it.
pub const BLOCKED_SUFFIX: &str = "Nothing is sent until you fix or clear the proxy in Settings and restart the app.";

/// How the app reaches the network this run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyMode {
    /// No proxy saved: requests go out directly.
    Direct,
    /// A usable saved proxy.
    Via(ProxyUrl),
    /// A proxy may be saved but cannot be used; the reason is shown to the user.
    Blocked(String),
}

impl ProxyMode {
    /// `Blocked` with `detail` followed by [`BLOCKED_SUFFIX`].
    pub fn blocked(detail: &str) -> ProxyMode {
        ProxyMode::Blocked(format!("{}. {}", detail.trim_end_matches('.'), BLOCKED_SUFFIX))
    }
}

/// The proxy settings `run()` read at startup, in effect until the next launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    /// Whether the trade client uses a `Via` proxy too (Settings `proxy_trade`).
    pub trade: bool,
}

impl ProxyConfig {
    /// No proxy for any transport: today's behaviour.
    pub fn none() -> Self {
        ProxyConfig { mode: ProxyMode::Direct, trade: true }
    }

    /// The proxy `transport` goes through, if any; `Err` with the reason when
    /// every transport is blocked, whatever `trade` says.
    pub fn for_transport(&self, transport: Transport) -> Result<Option<&ProxyUrl>, &str> {
        match (&self.mode, transport) {
            (ProxyMode::Blocked(reason), _) => Err(reason),
            (ProxyMode::Via(_), Transport::Trade) if !self.trade => Ok(None),
            (ProxyMode::Via(proxy), _) => Ok(Some(proxy)),
            (ProxyMode::Direct, _) => Ok(None),
        }
    }

    /// Why every request is blocked this run, if it is.
    pub fn blocked_reason(&self) -> Option<&str> {
        match &self.mode {
            ProxyMode::Blocked(reason) => Some(reason),
            _ => None,
        }
    }
}

/// The client builder every production client starts from. With no proxy for
/// `transport` it is a plain builder, so reqwest's system-proxy detection
/// stays as it was.
pub fn client_builder(proxy: &ProxyConfig, transport: Transport) -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    match proxy.for_transport(transport) {
        Err(reason) => builder.no_proxy().connector_layer(RefuseConnections(reason.into())),
        Ok(None) => builder,
        Ok(Some(proxy)) => builder.proxy(
            reqwest::Proxy::all(proxy.for_http_clients())
                .expect("validated by ProxyUrl::parse")
                .no_proxy(reqwest::NoProxy::from_string(REQWEST_NO_PROXY)),
        ),
    }
}

/// A connector layer that never calls the connector it wraps: every
/// connection fails with the blocked reason before a socket opens.
#[derive(Clone)]
struct RefuseConnections(std::sync::Arc<str>);

impl<S> tower_layer::Layer<S> for RefuseConnections {
    type Service = Refused<S>;

    fn layer(&self, inner: S) -> Refused<S> {
        Refused { _inner: inner, reason: self.0.clone() }
    }
}

/// Generic over the wrapped service because reqwest's connector is unnameable.
#[derive(Clone)]
struct Refused<S> {
    _inner: S,
    reason: std::sync::Arc<str>,
}

impl<S: tower_service::Service<R>, R> tower_service::Service<R> for Refused<S> {
    type Response = S::Response;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = std::future::Ready<Result<S::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _request: R) -> Self::Future {
        std::future::ready(Err(self.reason.to_string().into()))
    }
}

/// The WebView2 flags for `mode`: none when direct, the proxy and its local
/// bypass list for a usable proxy, a dead proxy and no host resolution when
/// blocked.
pub fn webview2_proxy_args(mode: &ProxyMode) -> Option<String> {
    match mode {
        ProxyMode::Direct => None,
        ProxyMode::Blocked(_) => Some(format!(
            "--proxy-server={} --proxy-bypass-list={} --host-resolver-rules=\"{}\"",
            BLOCKED_PROXY_SERVER, WEBVIEW2_BYPASS_LIST, BLOCKED_HOST_RESOLVER_RULES
        )),
        ProxyMode::Via(proxy) => Some(format!(
            "--proxy-server={} --proxy-bypass-list={}",
            proxy.server_for_webview2(),
            WEBVIEW2_BYPASS_LIST
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    fn proxy(raw: &str) -> ProxyUrl {
        ProxyUrl::parse(raw).unwrap().unwrap()
    }

    fn config(raw: &str) -> ProxyConfig {
        ProxyConfig { mode: ProxyMode::Via(proxy(raw)), trade: true }
    }

    #[test]
    fn proxy_url_accepts_the_three_schemes_with_optional_credentials() {
        for raw in [
            "http://127.0.0.1:8080",
            "https://proxy.example:443",
            "socks5://127.0.0.1:1080",
        ] {
            assert_eq!(ProxyUrl::parse(raw).unwrap().map(|p| p.0), Some(raw.to_string()), "{raw}");
        }
    }

    #[test]
    fn proxy_url_trims_surrounding_whitespace() {
        assert_eq!(proxy("  socks5://127.0.0.1:1080 ").as_str(), "socks5://127.0.0.1:1080");
    }

    #[test]
    fn proxy_url_empty_means_unset() {
        assert_eq!(ProxyUrl::parse(""), Ok(None));
        assert_eq!(ProxyUrl::parse("   "), Ok(None));
    }

    #[test]
    fn proxy_url_rejects_other_schemes_and_garbage() {
        for raw in ["socks4://127.0.0.1:1080", "ftp://proxy:21", "127.0.0.1:8080", "not a url", "http://"] {
            assert!(ProxyUrl::parse(raw).is_err(), "{raw} must be rejected");
        }
    }

    #[test]
    fn proxy_url_rejects_credentials_for_every_scheme() {
        for raw in [
            "http://user:pass@proxy.example:3128",
            "https://user@proxy.example:443",
            "socks5://user:pass@127.0.0.1:1080",
            "http://:pass@proxy.example:3128",
        ] {
            let err = ProxyUrl::parse(raw).expect_err(raw);
            assert!(err.contains("Proxy credentials are not supported: the app window (WebView2) cannot use them."), "{raw}: {err}");
        }
    }

    #[test]
    fn webview2_proxy_args_name_the_server_and_bypass_local_hosts() {
        assert_eq!(
            webview2_proxy_args(&ProxyMode::Via(proxy("http://proxy.example:3128"))),
            Some("--proxy-server=http://proxy.example:3128 --proxy-bypass-list=localhost;127.0.0.1;[::1];*.localhost".to_string())
        );
        assert_eq!(
            webview2_proxy_args(&ProxyMode::Via(proxy("socks5://127.0.0.1:1080"))),
            Some("--proxy-server=socks5://127.0.0.1:1080 --proxy-bypass-list=localhost;127.0.0.1;[::1];*.localhost".to_string())
        );
    }

    /// A listener that records the first request line it receives and answers
    /// 200, standing in for an HTTP proxy or an origin.
    fn recording_listener() -> (u16, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let _ = tx.send(request.lines().next().unwrap_or_default().to_string());
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok");
            }
        });
        (port, rx)
    }

    #[tokio::test]
    async fn client_with_proxy_sends_requests_through_it() {
        let (proxy_port, seen) = recording_listener();
        let client = client_builder(&config(&format!("http://127.0.0.1:{proxy_port}")), Transport::Other)
            .build()
            .unwrap();
        let body = client.get("http://profitofexile.top/api/status").send().await.unwrap().text().await.unwrap();
        assert_eq!(body, "ok");
        assert_eq!(seen.recv_timeout(Duration::from_secs(2)).unwrap(), "GET http://profitofexile.top/api/status HTTP/1.1");
    }

    /// With a proxy set, each local host goes direct: the proxy listener sees
    /// nothing. The request itself may fail (nothing listens on that name);
    /// only where it was sent matters.
    #[tokio::test]
    async fn client_with_proxy_bypasses_local_hosts() {
        let (proxy_port, seen) = recording_listener();
        let (origin_port, _origin) = recording_listener();
        let client = client_builder(&config(&format!("http://127.0.0.1:{proxy_port}")), Transport::Other)
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        for host in ["localhost", "127.0.0.1", "[::1]", "profitofexile.localhost"] {
            let _ = client.get(format!("http://{host}:{origin_port}/")).send().await;
            assert!(seen.recv_timeout(Duration::from_millis(300)).is_err(), "{host} went through the proxy");
        }
        // Control: the same client does use the proxy for a remote host.
        let _ = client.get("http://profitofexile.top/").send().await;
        assert!(seen.recv_timeout(Duration::from_secs(2)).is_ok());
    }

    #[test]
    fn socks5_proxy_builds_a_client() {
        assert!(client_builder(&config("socks5://127.0.0.1:1080"), Transport::Other).build().is_ok());
    }

    #[test]
    fn http_clients_resolve_dns_at_a_socks5_proxy() {
        assert_eq!(proxy("socks5://127.0.0.1:1080").for_http_clients(), "socks5h://127.0.0.1:1080");
        assert_eq!(proxy("SOCKS5://proxy.example:1080").for_http_clients(), "socks5h://proxy.example:1080");
        assert_eq!(proxy("http://proxy.example:3128").for_http_clients(), "http://proxy.example:3128");
        assert_eq!(proxy("https://proxy.example:443").for_http_clients(), "https://proxy.example:443");
    }

    #[test]
    fn socks5h_is_not_accepted_input() {
        assert!(ProxyUrl::parse("socks5h://127.0.0.1:1080").is_err());
    }

    /// A socks5 proxy answers the client greeting; the CONNECT request that
    /// follows names the target by domain (ATYP 0x03) only when DNS is left
    /// to the proxy.
    #[tokio::test]
    async fn socks5_client_sends_the_hostname_to_the_proxy() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else { return };
            let mut greeting = [0u8; 3];
            if stream.read_exact(&mut greeting).is_err() {
                return;
            }
            let _ = stream.write_all(&[0x05, 0x00]);
            let mut head = [0u8; 4];
            if stream.read_exact(&mut head).is_ok() {
                let _ = tx.send(head[3]);
            }
        });
        let client = client_builder(&config(&format!("socks5://127.0.0.1:{port}")), Transport::Other)
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let _ = client.get("http://profitofexile.top/").send().await;
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), 0x03, "address type must be a domain name");
    }

    #[test]
    fn trade_transport_skips_the_proxy_only_when_trade_is_off() {
        let url = proxy("http://proxy.example:3128");
        let on = ProxyConfig { mode: ProxyMode::Via(url.clone()), trade: true };
        let off = ProxyConfig { mode: ProxyMode::Via(url.clone()), trade: false };
        assert_eq!(on.for_transport(Transport::Trade), Ok(Some(&url)));
        assert_eq!(off.for_transport(Transport::Trade), Ok(None));
        assert_eq!(off.for_transport(Transport::Other), Ok(Some(&url)));
        assert_eq!(ProxyConfig::none().for_transport(Transport::Other), Ok(None));
    }

    /// Trade off: a trade client sends nothing to the proxy (the direct
    /// request may fail, only where it went matters); with trade on, the
    /// same request reaches the proxy.
    #[tokio::test]
    async fn trade_client_skips_the_proxy_when_trade_is_off() {
        let (proxy_port, seen) = recording_listener();
        let mut cfg = config(&format!("http://127.0.0.1:{proxy_port}"));
        cfg.trade = false;
        let direct = client_builder(&cfg, Transport::Trade).timeout(Duration::from_secs(2)).build().unwrap();
        let _ = direct.get("http://profitofexile.top/").send().await;
        assert!(seen.recv_timeout(Duration::from_millis(500)).is_err(), "trade went through the proxy with trade off");
        cfg.trade = true;
        let proxied = client_builder(&cfg, Transport::Trade).build().unwrap();
        let _ = proxied.get("http://profitofexile.top/").send().await;
        assert!(seen.recv_timeout(Duration::from_secs(2)).is_ok());
    }

    fn blocked_client() -> reqwest::Client {
        client_builder(&ProxyConfig { mode: ProxyMode::blocked("Proxy test reason"), trade: true }, Transport::Other)
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn blocked_proxy_client_opens_no_connection() {
        let (origin_port, seen) = recording_listener();
        let result = blocked_client().get(format!("http://127.0.0.1:{origin_port}/")).send().await;
        assert!(result.is_err(), "a blocked client got a response");
        assert!(seen.recv_timeout(Duration::from_millis(500)).is_err(), "a blocked client reached the origin");
    }

    #[tokio::test]
    async fn blocked_proxy_request_error_names_the_reason() {
        let (origin_port, _seen) = recording_listener();
        let err = blocked_client().get(format!("http://127.0.0.1:{origin_port}/")).send().await.unwrap_err();
        let mut chain = err.to_string();
        let mut source = std::error::Error::source(&err);
        while let Some(e) = source {
            chain.push_str(" | ");
            chain.push_str(&e.to_string());
            source = e.source();
        }
        assert!(
            chain.contains("Proxy test reason. Nothing is sent until you fix or clear the proxy in Settings and restart the app."),
            "{chain}"
        );
    }

    #[test]
    fn blocked_proxy_blocks_trade_even_with_trade_off() {
        assert_eq!(
            ProxyConfig { mode: ProxyMode::blocked("R"), trade: false }.for_transport(Transport::Trade),
            Err("R. Nothing is sent until you fix or clear the proxy in Settings and restart the app.")
        );
    }

    #[test]
    fn webview2_proxy_args_for_a_blocked_proxy_name_a_dead_proxy_and_resolve_no_host() {
        assert_eq!(
            webview2_proxy_args(&ProxyMode::blocked("R")),
            Some("--proxy-server=http://127.0.0.1:1 --proxy-bypass-list=localhost;127.0.0.1;[::1];*.localhost --host-resolver-rules=\"MAP * ~NOTFOUND , EXCLUDE localhost , EXCLUDE *.localhost\"".to_string())
        );
    }

    #[test]
    fn webview2_proxy_args_without_a_proxy_are_none() {
        assert_eq!(webview2_proxy_args(&ProxyMode::Direct), None);
    }

    // --- Guard: no client construction outside this module ---

    const CLIENT_CONSTRUCTORS: [&str; 4] = ["Client::new(", "Client::builder(", "ClientBuilder::new(", "reqwest::get("];

    /// `source` with every `#[cfg(test)]` item and every `//` comment line removed.
    fn production_code(source: &str) -> String {
        let without_comments: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut out = String::new();
        let mut rest = without_comments.as_str();
        while let Some(at) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..at]);
            let item = &rest[at + "#[cfg(test)]".len()..];
            let mut depth = 0usize;
            let mut end = item.len();
            for (i, c) in item.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            end = i + 1;
                            break;
                        }
                    }
                    ';' if depth == 0 => {
                        end = i + 1;
                        break;
                    }
                    _ => {}
                }
            }
            rest = &item[end..];
        }
        out.push_str(rest);
        out
    }

    /// The client constructors `code` calls, ignoring look-alikes such as
    /// `TradeApiClient::new(`.
    fn client_constructions(code: &str) -> Vec<&'static str> {
        CLIENT_CONSTRUCTORS
            .into_iter()
            .filter(|needle| {
                code.match_indices(needle).any(|(at, _)| {
                    !code[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_')
                })
            })
            .collect()
    }

    #[test]
    fn guard_flags_a_production_client_and_ignores_test_only_ones() {
        let source = "fn a() { let c = reqwest::Client::new(); }\n";
        assert_eq!(client_constructions(&production_code(source)), vec!["Client::new("]);
        let source = "fn a() { let t = TradeApiClient::new(); }\n#[cfg(test)]\nmod tests { fn b() { reqwest::Client::builder(); } }\n#[cfg(test)]\nfn c() { reqwest::get(\"x\"); }\nfn d() {}\n";
        assert!(client_constructions(&production_code(source)).is_empty());
        let source = "#[cfg(test)]\nuse x;\nfn d() { reqwest::get(\"x\"); }\n";
        assert_eq!(client_constructions(&production_code(source)), vec!["reqwest::get("]);
    }

    /// Walks the source tree at test time so a new file is covered without
    /// being listed here.
    #[test]
    fn no_production_source_builds_an_http_client_outside_the_factory() {
        fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, files);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        walk(&src, &mut files);
        assert!(files.len() > 10, "source walk found only {} files", files.len());
        let offenders: Vec<String> = files
            .iter()
            .filter(|path| !path.ends_with("http.rs"))
            .filter_map(|path| {
                let found = client_constructions(&production_code(&std::fs::read_to_string(path).unwrap()));
                (!found.is_empty()).then(|| format!("{}: {:?}", path.display(), found))
            })
            .collect();
        assert!(offenders.is_empty(), "build clients via http::client_builder: {offenders:#?}");
    }
}
