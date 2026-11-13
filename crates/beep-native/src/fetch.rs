//! URL 로 오디오 파일 내려받기.
//!
//! 파일을 바로 내려받을 수 있는 주소(예: `https://example.com/song.mp3`)만 받는다.
//! YouTube 같은 스트리밍 서비스 주소는 저작권 문제로 지원하지 않으며, 안내 문구로 거절한다.
//! (정책: `.agents/rules/project_architecture.md` 4.2절)

use std::time::Duration;

use crate::{Error, Result};

/// 내려받을 수 있는 최대 크기.
pub const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;
/// 내려받기 전체에 허용하는 시간.
const TIMEOUT: Duration = Duration::from_secs(120);
const MAX_REDIRECTS: u32 = 5;

/// 스트리밍 서비스 주소를 넣었을 때의 안내.
pub const STREAMING_NOT_SUPPORTED: &str = "YouTube 등 스트리밍 서비스 주소는 저작권 문제로 지원하지 않습니다. \
오디오 파일을 직접 내려받을 수 있는 주소(예: https://.../song.mp3)만 넣어 주세요. \
스트리밍 주소를 쓰고 싶다면 이 프로젝트를 포크(fork)해 직접 기능을 추가해 사용하시기 바랍니다.";

/// 파일을 직접 주지 않는 스트리밍 서비스의 도메인(하위 도메인 포함).
const STREAMING_HOSTS: [&str; 14] = [
    "youtube.com",
    "youtu.be",
    "youtube-nocookie.com",
    "soundcloud.com",
    "spotify.com",
    "music.apple.com",
    "tiktok.com",
    "vimeo.com",
    "twitch.tv",
    "melon.com",
    "genie.co.kr",
    "bugs.co.kr",
    "naver.com",
    "bandcamp.com",
];

/// 입력이 웹 주소인지(파일 경로가 아닌지).
pub fn is_url(input: &str) -> bool {
    let lower = input.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// 주소에서 호스트 이름만 뽑는다(소문자). 주소 형식이 아니면 `None`.
fn host_of(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next()?;
    let host = host_port.split(':').next()?.trim_end_matches('.');
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// 스트리밍 서비스의 주소인지.
pub fn is_streaming_service(url: &str) -> bool {
    host_of(url).is_some_and(|host| {
        STREAMING_HOSTS
            .iter()
            .any(|known| host == *known || host.ends_with(&format!(".{known}")))
    })
}

/// 주소에서 오디오 파일을 내려받는다.
pub fn download(url: &str) -> Result<Vec<u8>> {
    let url = url.trim();
    if !is_url(url) {
        return Err(Error(
            "http:// 또는 https:// 로 시작하는 주소만 받을 수 있습니다.".into(),
        ));
    }
    if is_streaming_service(url) {
        return Err(Error(STREAMING_NOT_SUPPORTED.into()));
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(MAX_REDIRECTS)
        .build()
        .into();
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| Error(format!("음악을 내려받지 못했습니다: {e}")))?;

    // 웹 페이지가 돌아왔다면 파일의 직접 주소가 아니다.
    if response
        .body()
        .mime_type()
        .is_some_and(|mime| mime.eq_ignore_ascii_case("text/html"))
    {
        return Err(Error(format!(
            "이 주소는 오디오 파일이 아니라 웹 페이지입니다. {STREAMING_NOT_SUPPORTED}"
        )));
    }
    let too_large = || {
        Error(format!(
            "파일이 너무 큽니다 (최대 {} MB).",
            MAX_DOWNLOAD_BYTES / 1024 / 1024
        ))
    };
    if response
        .body()
        .content_length()
        .is_some_and(|length| length > MAX_DOWNLOAD_BYTES)
    {
        return Err(too_large());
    }

    response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD_BYTES)
        .read_to_vec()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => too_large(),
            other => Error(format!("음악을 내려받는 중 끊겼습니다: {other}")),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_urls() {
        assert!(is_url("https://example.com/a.mp3"));
        assert!(is_url("  HTTP://example.com/a.mp3"));
        assert!(!is_url("C:\\music\\a.mp3"));
        assert!(!is_url("music/a.mp3"));
        assert!(!is_url("ftp://example.com/a.mp3"));
    }

    #[test]
    fn extracts_hosts() {
        assert_eq!(
            host_of("https://Example.com/a.mp3").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            host_of("https://user:pw@cdn.example.com:8443/a?x=1").as_deref(),
            Some("cdn.example.com")
        );
        assert_eq!(
            host_of("https://example.com").as_deref(),
            Some("example.com")
        );
        assert_eq!(host_of("not a url"), None);
    }

    #[test]
    fn streaming_services_are_detected() {
        for url in [
            "https://www.youtube.com/watch?v=abc",
            "https://youtu.be/abc",
            "https://music.youtube.com/watch?v=abc",
            "https://soundcloud.com/artist/track",
            "https://open.spotify.com/track/abc",
        ] {
            assert!(is_streaming_service(url), "{url}");
        }
        for url in [
            "https://example.com/song.mp3",
            "https://notyoutube.com/a.mp3",
            "https://example.com/youtube.com/a.mp3",
        ] {
            assert!(!is_streaming_service(url), "{url}");
        }
    }

    #[test]
    fn streaming_urls_are_refused_with_guidance() {
        let error = download("https://www.youtube.com/watch?v=abc").unwrap_err();
        assert!(error.0.contains("포크"), "{error}");
    }

    #[test]
    fn non_urls_are_refused() {
        assert!(download("C:\\music\\a.mp3").is_err());
    }
}
