//! 로컬 AI(Ollama)에 가수와 곡 제목 묻기.
//!
//! 음악 파일의 태그와 파일 이름 규칙으로 알아내지 못했을 때만 쓴다. 이 컴퓨터에서 돌고 있는
//! Ollama 에 **파일 이름만** 보내며, 다른 곳으로는 아무것도 보내지 않는다.
//! Ollama 가 없거나 꺼져 있으면 조용히 "모름" 으로 돌아간다.
//! (정책: `.agents/rules/project_architecture.md` 4.5절)

use std::time::Duration;

use beep_core::SongInfo;
use beep_core::naming::file_stem;
use serde_json::{Value, json};

use crate::{Error, Result};

/// Ollama 의 기본 주소.
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434";
/// Ollama 주소를 바꾸는 환경 변수(Ollama 자체가 쓰는 것과 같다).
const HOST_ENV: &str = "OLLAMA_HOST";
/// 쓸 모델을 지정하는 환경 변수. 없으면 설치된 모델 중 첫 번째를 쓴다.
const MODEL_ENV: &str = "BEEP_GEN_AI_MODEL";

/// Ollama 가 켜져 있는지 확인할 때 기다리는 시간. 없을 때 사용자를 기다리게 하지 않는다.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
/// 답을 기다리는 시간. 모델을 처음 불러올 때는 수십 초가 걸릴 수 있다.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(90);

const SYSTEM_PROMPT: &str = "You extract the artist and the song title from a music file name. \
Answer with JSON only: {\"artist\": \"...\", \"title\": \"...\"}.\n\
Rules:\n\
- The title must be text that is written in the file name. Never invent a title.\n\
- Remove track numbers, uploader or channel names, and tags such as \"Official Video\", \"MV\", \"Lyrics\", \"가사\", \"HD\", \"4K\".\n\
- If the file name does not contain the artist, give the artist only when you are certain who performs this song; otherwise use an empty string.\n\
- If you cannot tell what the title is, use empty strings for both.\n\
- Keep the original language and spelling. Do not translate.";

/// 로컬 AI 의 답.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAnswer {
    pub song: SongInfo,
    /// 답한 모델의 이름(예: `gemma3:4b`).
    pub model: String,
}

/// 환경 변수에서 Ollama 주소를 읽는다. 형식은 `host`, `host:port`, `http://host:port` 모두 받는다.
fn base_url() -> String {
    let Ok(host) = std::env::var(HOST_ENV) else {
        return DEFAULT_BASE_URL.to_owned();
    };
    normalize_base_url(&host)
}

fn normalize_base_url(host: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    if host.is_empty() {
        return DEFAULT_BASE_URL.to_owned();
    }
    let with_scheme = if host.contains("://") {
        host.to_owned()
    } else {
        format!("http://{host}")
    };
    // 서버가 "모든 주소에서 받기"(0.0.0.0)로 설정돼 있어도 접속은 이 컴퓨터로 한다.
    let with_scheme = with_scheme.replace("://0.0.0.0", "://127.0.0.1");
    let authority = with_scheme.split_once("://").map_or("", |(_, rest)| rest);
    if authority.contains(':') {
        with_scheme
    } else {
        format!("{with_scheme}:11434")
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build()
        .into()
}

/// 쓸 모델을 고른다. Ollama 가 꺼져 있거나 설치된 모델이 없으면 `None`.
fn pick_model(base: &str, wanted: Option<&str>) -> Option<String> {
    let body = agent(PROBE_TIMEOUT)
        .get(&format!("{base}/api/tags"))
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let listing: Value = serde_json::from_str(&body).ok()?;
    let names: Vec<&str> = listing["models"]
        .as_array()?
        .iter()
        .filter_map(|model| model["name"].as_str())
        .collect();
    if let Some(wanted) = wanted.map(str::trim).filter(|w| !w.is_empty()) {
        // 지정한 모델이 있으면 그것만 쓴다. 태그(`:4b`)를 빼고 적어도 찾는다.
        return names
            .iter()
            .find(|name| **name == wanted || name.split(':').next() == Some(wanted))
            .map(|name| (*name).to_owned());
    }
    names
        .into_iter()
        // 글을 쓰지 못하는 임베딩 전용 모델은 건너뛴다.
        .find(|name| !name.to_lowercase().contains("embed"))
        .map(str::to_owned)
}

/// 비교용으로 글자와 숫자만 남기고 소문자로 바꾼다.
fn letters_only(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// 로컬 AI 에 파일 이름을 보여주고 가수와 곡 제목을 묻는다.
///
/// Ollama 를 쓸 수 없거나 AI 가 모르겠다고 하면 `Ok(None)`.
/// 요청이 중간에 실패하면 이유를 `Err` 로 돌려준다(사용자에게 알릴 수 있게).
pub fn identify_song(file_name: &str) -> Result<Option<AiAnswer>> {
    let wanted = std::env::var(MODEL_ENV).ok();
    identify_song_at(&base_url(), wanted.as_deref(), file_name)
}

fn identify_song_at(base: &str, wanted: Option<&str>, file_name: &str) -> Result<Option<AiAnswer>> {
    let stem = file_stem(file_name).trim();
    if stem.is_empty() {
        return Ok(None);
    }
    let Some(model) = pick_model(base, wanted) else {
        return Ok(None);
    };

    let request = json!({
        "model": model,
        "stream": false,
        "options": { "temperature": 0 },
        "format": {
            "type": "object",
            "properties": {
                "artist": { "type": "string" },
                "title": { "type": "string" },
            },
            "required": ["artist", "title"],
        },
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": format!("File name: {stem}") },
        ],
    });
    let failed = |reason: String| Error(format!("로컬 AI({model})에 묻지 못했습니다: {reason}"));
    let body = agent(ANSWER_TIMEOUT)
        .post(&format!("{base}/api/chat"))
        .header("Content-Type", "application/json")
        .send(request.to_string())
        .map_err(|e| failed(e.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|e| failed(e.to_string()))?;

    let reply: Value = serde_json::from_str(&body).map_err(|e| failed(e.to_string()))?;
    let content = reply["message"]["content"].as_str().unwrap_or_default();
    // 형식에 맞지 않는 답은 "모름" 으로 본다.
    let Ok(answer) = serde_json::from_str::<Value>(content) else {
        return Ok(None);
    };
    let song = SongInfo::new(answer["artist"].as_str(), answer["title"].as_str());

    // AI 가 파일 이름에 없는 제목을 지어냈으면 버린다.
    // 파일 이름을 그대로 되돌려준 답(가수 없이 제목 = 이름)도 알아낸 것이 없으므로 버린다.
    let name_letters = letters_only(stem);
    let useful = song
        .title
        .as_deref()
        .map(letters_only)
        .is_some_and(|title| {
            !title.is_empty()
                && name_letters.contains(&title)
                && (song.artist.is_some() || title != name_letters)
        });
    Ok(useful.then_some(AiAnswer { song, model }))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    use super::*;

    /// 가짜 Ollama. 모델 목록과, 질문에 대한 정해진 답을 돌려준다.
    fn fake_ollama(models: &'static [&'static str], answer: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();

                let reply = if request_line.starts_with("GET /api/tags") {
                    let list: Vec<Value> = models.iter().map(|m| json!({ "name": m })).collect();
                    json!({ "models": list }).to_string()
                } else {
                    let asked: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(asked["stream"], false);
                    assert!(
                        asked["messages"][1]["content"]
                            .as_str()
                            .unwrap()
                            .contains("File name:")
                    );
                    json!({ "model": asked["model"], "message": { "role": "assistant", "content": answer } })
                        .to_string()
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .unwrap();
            }
        });
        base
    }

    #[test]
    fn asks_the_first_text_model_and_reads_the_answer() {
        let base = fake_ollama(
            &["nomic-embed-text:latest", "gemma3:4b", "other:1b"],
            r#"{"artist": "아이유", "title": "Blueming"}"#,
        );
        let answer = identify_song_at(&base, None, "[MV] IU(아이유) _ Blueming(블루밍).mp3")
            .unwrap()
            .unwrap();
        assert_eq!(answer.model, "gemma3:4b");
        assert_eq!(answer.song, SongInfo::new(Some("아이유"), Some("Blueming")));
    }

    #[test]
    fn a_wanted_model_is_used_only_if_installed() {
        let base = fake_ollama(
            &["gemma3:4b", "other:1b"],
            r#"{"artist":"","title":"Song"}"#,
        );
        let answer = identify_song_at(&base, Some("other"), "song (live at home).mp3")
            .unwrap()
            .unwrap();
        assert_eq!(answer.model, "other:1b");
        assert_eq!(answer.song, SongInfo::new(None, Some("Song")));
        assert_eq!(
            identify_song_at(&base, Some("missing"), "song (live at home).mp3").unwrap(),
            None
        );
    }

    #[test]
    fn invented_or_empty_answers_are_dropped() {
        for answer in [
            r#"{"artist": "Queen", "title": "Bohemian Rhapsody"}"#,
            r#"{"artist": "", "title": ""}"#,
            r#"{"artist": "", "title": "Track07"}"#,
            "sorry, I do not know",
        ] {
            let base = fake_ollama(&["gemma3:4b"], answer);
            assert_eq!(
                identify_song_at(&base, None, "track07.mp3").unwrap(),
                None,
                "{answer}"
            );
        }
    }

    #[test]
    fn missing_ollama_or_models_means_no_answer() {
        // 아무도 듣지 않는 포트.
        let unused = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", unused.local_addr().unwrap());
        drop(unused);
        assert_eq!(identify_song_at(&base, None, "a - b.mp3").unwrap(), None);

        let base = fake_ollama(&[], "{}");
        assert_eq!(identify_song_at(&base, None, "a - b.mp3").unwrap(), None);
    }

    #[test]
    fn normalizes_host_settings() {
        let cases = [
            ("", DEFAULT_BASE_URL),
            ("localhost", "http://localhost:11434"),
            ("0.0.0.0:8080", "http://127.0.0.1:8080"),
            ("http://192.168.0.5:11434/", "http://192.168.0.5:11434"),
            ("https://ai.local", "https://ai.local:11434"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalize_base_url(input), expected, "{input}");
        }
    }
}
