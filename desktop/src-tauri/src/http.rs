//! The one place this crate builds an HTTP client (POE-360).
//!
//! Every `reqwest::Client` in production comes from [`client_builder`], so the
//! user's proxy setting reaches every request without per-call wiring. The
//! source-scan test at the bottom fails when a client is built anywhere else.

/// Hosts that never go through the proxy: the local dev server
/// (`*.localhost`) and loopback. Two spellings of the same set, one per
/// transport's syntax.
const REQWEST_NO_PROXY: &str = "localhost,127.0.0.1,::1,.localhost";
const WEBVIEW2_BYPASS_LIST: &str = "localhost;127.0.0.1;[::1];*.localhost";

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

/// The client builder every production client starts from. With no proxy it
/// is a plain builder, so reqwest's system-proxy detection stays as it was.
pub fn client_builder(proxy: Option<&ProxyUrl>) -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder();
    match proxy {
        None => builder,
        Some(proxy) => builder.proxy(
            reqwest::Proxy::all(proxy.as_str())
                .expect("validated by ProxyUrl::parse")
                .no_proxy(reqwest::NoProxy::from_string(REQWEST_NO_PROXY)),
        ),
    }
}

/// The WebView2 flags that route every webview through `proxy`.
pub fn webview2_proxy_args(proxy: &ProxyUrl) -> String {
    format!(
        "--proxy-server={} --proxy-bypass-list={}",
        proxy.server_for_webview2(),
        WEBVIEW2_BYPASS_LIST
    )
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
            webview2_proxy_args(&proxy("http://proxy.example:3128")),
            "--proxy-server=http://proxy.example:3128 --proxy-bypass-list=localhost;127.0.0.1;[::1];*.localhost"
        );
        assert_eq!(
            webview2_proxy_args(&proxy("socks5://127.0.0.1:1080")),
            "--proxy-server=socks5://127.0.0.1:1080 --proxy-bypass-list=localhost;127.0.0.1;[::1];*.localhost"
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
        let client = client_builder(Some(&proxy(&format!("http://127.0.0.1:{proxy_port}"))))
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
        let client = client_builder(Some(&proxy(&format!("http://127.0.0.1:{proxy_port}"))))
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
        assert!(client_builder(Some(&proxy("socks5://127.0.0.1:1080"))).build().is_ok());
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
