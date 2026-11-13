//! Gem art from the server's `/api/icon/gems/{gem}` route, cached as one PNG
//! per gem in a caller-chosen directory.
//!
//! Shared by the mercenary seed (`<icons>/seed/`, under its own epoch lock) and
//! the lab gem reader (`<app_data>/gem-art/`). Requests go through the caller's
//! `state.server_http` client.

use std::path::{Path, PathBuf};
use std::time::Duration;

use image::RgbaImage;

/// Request timeout for one gem-art call.
///
/// The same 20 s the merc pool sync gives a pool call, and for the same reason —
/// the shared HTTP client sets none. It has to clear the server's own 10 s
/// poewiki timeout, which is what a DEV server spends on the first request for
/// an icon its cache volume does not hold yet; prod answers from that volume.
const ART_TIMEOUT: Duration = Duration::from_secs(20);

/// What one gem-art request came back with, before it is judged.
///
/// Two variants rather than a `Result<Response>` because the transport failure
/// and the server's answer are different EVIDENCE even though they get the same
/// verdict — the log line names which one happened.
#[derive(Debug, Clone, PartialEq)]
pub enum ArtReply {
    /// No response at all — offline, DNS, TLS, a timeout.
    Unreachable(String),
    /// An HTTP status and whatever body came with it.
    Response(u16, Vec<u8>),
}

/// Accept one reply as cacheable art, or say why the gem stays without art.
///
/// **Everything that is not a 200 carrying a decodable PNG is unavailable, and
/// unavailable means nothing is written.** That absence IS the retry: the
/// caller's next pass finds no cached file and asks again. 404 (the gem is not
/// in the server's embedded map) and 502 (the server holds the name but could
/// not produce bytes) deliberately collapse into the same answer — both are the
/// server's, both can be temporary on a dev server that fetches the wiki live,
/// and neither is something this device can act on differently.
///
/// The format is pinned to PNG rather than sniffed: the route serves PNG, and
/// a proxy's HTML error page decodes as nothing, but a captive-portal login
/// screen served as a JPEG would decode as an image and cache as gem art.
pub fn accept_art(reply: ArtReply) -> Result<Vec<u8>, String> {
    let (status, body) = match reply {
        ArtReply::Unreachable(why) => return Err(format!("unreachable — {why}")),
        ArtReply::Response(status, body) => (status, body),
    };
    if status != 200 {
        return Err(format!("server returned {status}"));
    }
    match image::load_from_memory_with_format(&body, image::ImageFormat::Png) {
        Ok(_) => Ok(body),
        Err(e) => Err(format!("not a readable PNG — {e}")),
    }
}

/// `{server}/api/icon/gems/{gem}`, with the gem percent-encoded as one path
/// segment.
///
/// `server_url` carries no `/api` suffix — the JS `apiBase` does, and a URL
/// built by pasting the two together 404s. The segment goes through the URL
/// parser rather than a `format!` because every gem name has spaces in it and
/// four have none of the other characters that would make a hand-rolled
/// encoder look wrong until the map grows one.
///
/// `None` when `server` is not a parseable absolute URL, which is a
/// misconfigured device, not a failed fetch — the caller skips the pass.
pub fn art_url(server: &str, gem: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(server).ok()?;
    url.path_segments_mut().ok()?.pop_if_empty().extend(["api", "icon", "gems", gem]);
    Some(url.to_string())
}

/// File-name-safe form of a gem name — the cache file's stem and the fixture's.
pub fn art_slug(gem: &str) -> String {
    let s: String = gem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    if s.is_empty() {
        "unnamed".to_string()
    } else {
        s
    }
}

/// `<dir>/<slug>.png` — where one gem's art is cached.
pub fn art_file(dir: &Path, gem: &str) -> PathBuf {
    dir.join(format!("{}.png", art_slug(gem)))
}

/// Request one gem's art. `None` when `server` is not a usable address (see
/// [`art_url`]); every other outcome is an [`ArtReply`] for [`accept_art`].
pub async fn fetch_art(http: &reqwest::Client, server: &str, gem: &str) -> Option<ArtReply> {
    let url = art_url(server, gem)?;
    Some(match http.get(&url).timeout(ART_TIMEOUT).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            match response.bytes().await {
                Ok(body) => ArtReply::Response(status, body.to_vec()),
                Err(e) => ArtReply::Unreachable(e.to_string()),
            }
        }
        Err(e) => ArtReply::Unreachable(e.to_string()),
    })
}

/// Write one gem's art to `<dir>/<slug>.png`, creating `dir`.
///
/// Temp-then-rename: a half-written PNG would read as cached and never be
/// fetched again. Callers that race a cache reset hold their own lock around
/// this call.
pub fn write_art(dir: &Path, gem: &str, bytes: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = art_file(dir, gem);
    let tmp = path.with_extension("png.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

/// Decode one cached art file.
///
/// A file that is present but does not decode is DELETED, not just skipped: it
/// reads as cached forever otherwise, so the gem would never get art and never
/// be re-fetched either. A truncated download and a half-written file are the
/// two ways to get one, and both want the same answer — ask again next pass.
pub fn read_art(dir: &Path, gem: &str) -> Option<RgbaImage> {
    let path = art_file(dir, gem);
    let bytes = std::fs::read(&path).ok()?;
    match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
        Ok(img) => Some(img.to_rgba8()),
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    /// Fetched (never committed — see the fixture README) gem art, fetched over
    /// the production route (`/api/icon/gems/{gem}`).
    const ART_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/merc-seed-art");

    /// How to get the art, named in every failure that misses it.
    const FETCH_HINT: &str = "the seed art is not committed (GPL-3 repo, \
        CC-BY-NC-SA icons — see the fixture README); fetch it with \
        `make merc-seed-art`";

    /// The bytes of a real gem art file — the only PNG in this suite that is
    /// certainly what the route serves.
    fn art_bytes(gem: &str) -> Vec<u8> {
        let path = std::path::Path::new(ART_DIR).join(format!("{}.png", art_slug(gem)));
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e} — {FETCH_HINT}", path.display()))
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gem-art-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The cache file is named by the gem's slug.
    #[test]
    fn art_is_cached_under_the_gems_slug() {
        assert_eq!(art_slug("Added Chaos Damage Support"), "added-chaos-damage-support");
        assert_eq!(
            art_file(Path::new("/x/gem-art"), "Fork Support"),
            Path::new("/x/gem-art/fork-support.png"),
        );
    }

    /// A server that cannot be reached at all leaves the gem without art.
    #[test]
    fn an_unreachable_server_yields_no_art() {
        let why = accept_art(ArtReply::Unreachable("dns error".into()))
            .expect_err("an unreachable server produced art");

        assert!(why.contains("dns error"), "{why}");
    }

    /// A gem the server's map does not carry answers 404, and 404 is not art.
    #[test]
    fn a_404_yields_no_art() {
        let why = accept_art(ArtReply::Response(404, b"not found".to_vec()))
            .expect_err("a 404 produced art");

        assert!(why.contains("404"), "{why}");
    }

    /// A 502 — the server holds the name but could not produce bytes — is the
    /// same non-answer as a 404, and must not be cached as art.
    #[test]
    fn a_502_yields_no_art() {
        let why = accept_art(ArtReply::Response(502, art_bytes("Fork Support")))
            .expect_err("a 502 produced art");

        assert!(why.contains("502"), "{why}");
    }

    /// A 200 carrying something that is not a PNG is refused. A proxy's error
    /// page arrives exactly like this, and caching it would give the gem a
    /// picture of an error.
    #[test]
    fn a_body_that_is_not_a_png_yields_no_art() {
        let why = accept_art(ArtReply::Response(200, b"<html>login</html>".to_vec()))
            .expect_err("an HTML body produced art");

        assert!(why.contains("PNG"), "{why}");
    }

    /// A 200 carrying a JPEG is refused too. This is the case the FORMAT PIN
    /// buys: a sniffing decoder would take it, and the gem would get whatever a
    /// captive portal or a mis-configured proxy served.
    #[test]
    fn a_body_that_is_a_jpeg_yields_no_art() {
        let mut jpeg = Vec::new();
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 16, image::Rgba([90, 40, 20, 255])))
            .to_rgb8()
            .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .expect("encode a JPEG");

        let why = accept_art(ArtReply::Response(200, jpeg)).expect_err("a JPEG produced art");

        assert!(why.contains("PNG"), "{why}");
    }

    /// The accepted path hands back the bytes AS SERVED — the cache holds the
    /// file the route sent, not a re-encoding of it.
    #[test]
    fn a_200_carrying_a_png_is_accepted_byte_for_byte() {
        let bytes = art_bytes("Fork Support");

        let accepted = accept_art(ArtReply::Response(200, bytes.clone())).expect("accepted");

        assert_eq!(accepted, bytes);
    }

    /// The gem is one percent-encoded path segment under `/api/icon/gems`, and
    /// `server_url` carries no `/api` of its own.
    #[test]
    fn the_art_url_encodes_the_gem_under_the_api_prefix() {
        assert_eq!(
            art_url("https://profitofexile.top", "Added Chaos Damage Support").as_deref(),
            Some("https://profitofexile.top/api/icon/gems/Added%20Chaos%20Damage%20Support"),
        );
        // A trailing slash on the configured server must not double up.
        assert_eq!(
            art_url("https://profitofexile.top/", "Fork Support").as_deref(),
            Some("https://profitofexile.top/api/icon/gems/Fork%20Support"),
        );
    }

    /// Art written to a directory that does not exist yet reads back as the
    /// same picture, with no temp file left behind.
    #[test]
    fn written_art_reads_back_from_the_given_dir() {
        let dir = tmp_dir("roundtrip").join("nested");
        let bytes = art_bytes("Fork Support");

        write_art(&dir, "Fork Support", &bytes).expect("write");

        let read = read_art(&dir, "Fork Support").expect("the written art did not read back");
        let expected = image::load_from_memory(&bytes).expect("decode").to_rgba8();
        assert_eq!(read, expected);
        assert!(!art_file(&dir, "Fork Support").with_extension("png.tmp").exists());
    }

    /// Cached art that does not decode is DELETED, so the next pass fetches it
    /// again. Left in place it would read as cached forever.
    #[test]
    fn a_corrupt_cache_file_is_dropped_so_the_next_pass_refetches() {
        let dir = tmp_dir("corrupt");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(art_file(&dir, "Fork Support"), b"truncated").expect("write");

        assert!(read_art(&dir, "Fork Support").is_none(), "a truncated file decoded");
        assert!(
            !art_file(&dir, "Fork Support").exists(),
            "the unreadable file was left to read as cached forever",
        );
    }
}
