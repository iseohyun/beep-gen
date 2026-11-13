//! 음원 하나를 만드는 전체 절차.
//!
//! "음악을 가져오고 → 안내 음성을 만들고 → 코어로 음원을 만든다" 는 순서는
//! CLI·데스크톱 앱·MCP 가 똑같다. 그 순서를 여기 한 곳에 둔다.
//! 소리를 만드는 계산은 코어가 하고, 여기서는 가져오기와 순서만 맡는다.

use std::path::PathBuf;

use beep_core::{
    MusicName, MusicReport, OutputFormat, SongInfo, TrackInputs, TrackParams, build_track,
    intro_text, output_file_name,
};

use crate::song::{self, IdentifiedBy};
use crate::{fetch, tts};

/// 배경 음악을 어디서 가져올지.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MusicSource {
    /// 이 컴퓨터의 파일.
    Path(PathBuf),
    /// 오디오 파일의 직접 주소(http/https).
    Url(String),
    /// 이미 읽어 둔 파일 내용.
    Bytes(Vec<u8>),
}

impl MusicSource {
    /// 사용자가 적은 문자열을 파일 경로 또는 주소로 해석한다.
    pub fn parse(input: &str) -> Self {
        if fetch::is_url(input) {
            MusicSource::Url(input.trim().to_owned())
        } else {
            MusicSource::Path(PathBuf::from(input))
        }
    }
}

/// 시작 안내 음성 요청.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntroRequest {
    /// 읽을 문구. `{n}`, `{m}`, `{l}` 은 설정값으로 바뀐다. 비어 있으면 안내 음성을 넣지 않는다.
    pub template: String,
    /// 쓸 음성의 식별자 또는 이름(일부만 적어도 된다). 생략하면 문구의 언어에 맞게 고른다.
    pub voice: Option<String>,
}

/// 결과 파일의 이름에 넣을 가수와 곡을 어떻게 정할지.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SongLookup {
    /// 요청에 적힌 것만 쓴다(화면에서 사용자가 이미 확인한 경우, 이름이 필요 없는 경우).
    #[default]
    Given,
    /// 요청에 없는 것은 음악 파일의 태그와 파일 이름에서 찾는다.
    Detect,
    /// 그래도 모르면 로컬 AI 에 묻는다.
    DetectWithAi,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobRequest {
    pub params: TrackParams,
    pub format: OutputFormat,
    pub music: Option<MusicSource>,
    pub intro: Option<IntroRequest>,
    /// 사용자가 알려준 가수와 곡. 모르는 쪽은 비워 둔다.
    pub song: SongInfo,
    /// 원래 음악 파일의 이름. 생략하면 경로나 주소에서 얻는다.
    pub music_name: Option<String>,
    pub song_lookup: SongLookup,
}

impl JobRequest {
    /// 비프만 만드는 기본 요청. 필요한 항목만 바꿔 쓴다.
    pub fn new(params: TrackParams, format: OutputFormat) -> Self {
        Self {
            params,
            format,
            music: None,
            intro: None,
            song: SongInfo::default(),
            music_name: None,
            song_lookup: SongLookup::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobOutput {
    /// 완성된 파일 바이트.
    pub bytes: Vec<u8>,
    /// 규칙에 따라 지은 파일 이름. 예: `[1.4초] 가수 - 곡.mp3`
    pub file_name: String,
    /// 파일 이름에 쓴 가수와 곡. 몰라서 원래 파일 이름을 썼으면 비어 있다.
    pub song: SongInfo,
    /// 배경 음악을 넣었을 때의 보고.
    pub music_report: Option<MusicReport>,
    /// 맨 앞 안내 구간의 길이(초). 안내 음성이 없으면 0.
    pub intro_seconds: f64,
    /// 음원은 만들었지만 사용자가 알아야 할 일(예: 안내 음성을 넣지 못함).
    pub notices: Vec<String>,
}

/// 실패의 종류. 호출자가 사용자에게 다르게 안내할 수 있도록 나눈다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobError {
    /// 입력이 잘못됐다: 범위를 벗어난 설정값, 읽을 수 없는 음악, 박자를 찾지 못한 곡 등.
    Input(String),
    /// 가져오지 못했다: 없는 파일, 네트워크 오류, 지원하지 않는 주소 등.
    Fetch(String),
}

impl std::fmt::Display for JobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobError::Input(message) | JobError::Fetch(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for JobError {}

/// 음악을 내려받는 동안 알리는 진행 단계 설명.
pub const DOWNLOAD_STAGE: &str = "음악을 내려받는 중";

/// 음원을 만든다. `on_progress(백분율, 단계 설명)` 은 진행 상황이 바뀔 때마다 호출된다.
pub fn run(
    request: &JobRequest,
    mut on_progress: impl FnMut(u8, &str),
) -> Result<JobOutput, JobError> {
    // 가져오기 전에 설정값부터 확인해, 잘못된 값 때문에 내려받기를 헛되이 하지 않는다.
    request
        .params
        .validate()
        .map_err(|e| JobError::Input(e.to_string()))?;

    let music = match &request.music {
        None => None,
        Some(MusicSource::Bytes(bytes)) => Some(bytes.clone()),
        Some(MusicSource::Path(path)) => Some(std::fs::read(path).map_err(|e| {
            JobError::Fetch(format!("{} 을(를) 읽지 못했습니다: {e}", path.display()))
        })?),
        Some(MusicSource::Url(url)) => {
            on_progress(0, DOWNLOAD_STAGE);
            Some(fetch::download(url).map_err(|e| JobError::Fetch(e.to_string()))?)
        }
    };

    let mut notices = Vec::new();
    let music_name = music
        .as_deref()
        .map(|bytes| name_music(request, bytes, &mut notices));

    // 안내 음성을 만들지 못해도 음원은 만든다. 대신 그 사실을 알린다.
    let intro = request.intro.as_ref().and_then(|intro| {
        let text = intro_text(&intro.template, &request.params);
        if text.is_empty() {
            return None;
        }
        let voice = match intro.voice.as_deref() {
            Some(wanted) => find_voice(wanted).map(Some),
            None => Ok(None),
        };
        match voice.and_then(|voice| tts::synthesize(&text, voice.as_deref())) {
            Ok(wav) => Some(wav),
            Err(e) => {
                notices.push(format!("안내 음성 없이 만들었습니다. {e}"));
                None
            }
        }
    });

    let inputs = TrackInputs {
        music: music.as_deref(),
        intro: intro.as_deref(),
    };
    let mut last = None;
    let output = build_track(&request.params, request.format, inputs, |progress| {
        if last != Some(progress) {
            last = Some(progress);
            on_progress(progress.percent, progress.stage.label());
        }
    })
    .map_err(|e| JobError::Input(e.to_string()))?;

    Ok(JobOutput {
        bytes: output.bytes,
        file_name: output_file_name(&request.params, request.format, music_name.as_ref()),
        song: music_name.map(|name| name.song).unwrap_or_default(),
        music_report: output.music,
        intro_seconds: output.intro_seconds,
        notices,
    })
}

/// 결과 파일의 이름에 쓸, 배경 음악에 대해 아는 것을 모은다.
fn name_music(request: &JobRequest, bytes: &[u8], notices: &mut Vec<String>) -> MusicName {
    let source_name = request
        .music_name
        .clone()
        .or_else(|| match request.music.as_ref()? {
            MusicSource::Path(path) => Some(path.file_name()?.to_string_lossy().into_owned()),
            MusicSource::Url(url) => fetch::file_name_of(url),
            MusicSource::Bytes(_) => None,
        });
    let mut song = request.song.clone().cleaned();

    // 사용자가 곡 제목을 알려줬으면 더 찾지 않는다.
    let ask_ai = request.song_lookup == SongLookup::DetectWithAi;
    if request.song_lookup != SongLookup::Given && song.title.is_none() {
        let found = song::identify(Some(bytes), source_name.as_deref(), ask_ai);
        if let Some(problem) = found.problem {
            notices.push(format!(
                "{problem} 파일 이름은 원래 음악 파일의 이름으로 지었습니다."
            ));
        }
        if found.by == Some(IdentifiedBy::Ai) {
            let label = [found.song.artist.as_deref(), found.song.title.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" - ");
            notices.push(format!(
                "가수와 곡 이름은 로컬 AI({})가 파일 이름에서 추정했습니다: {label}. 틀릴 수 있으니 확인해 주세요.",
                found.model.as_deref().unwrap_or("Ollama")
            ));
        }
        song = SongInfo {
            artist: song.artist.or(found.song.artist),
            title: found.song.title,
        };
    }
    MusicName { song, source_name }
}

/// 식별자 또는 이름(일부)으로 음성을 찾아 식별자를 돌려준다.
pub fn find_voice(wanted: &str) -> crate::Result<String> {
    let needle = wanted.trim().to_lowercase();
    tts::list_voices()?
        .into_iter()
        .find(|voice| voice.id == wanted || voice.name.to_lowercase().contains(&needle))
        .map(|voice| voice.id)
        .ok_or_else(|| crate::Error(format!("음성을 찾지 못했습니다: {wanted}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> JobRequest {
        JobRequest::new(
            TrackParams {
                length_sec: 1,
                ..TrackParams::default()
            },
            OutputFormat::Wav,
        )
    }

    /// 박자가 있는 8초짜리 음악 파일의 내용.
    fn music_bytes() -> Vec<u8> {
        let params = TrackParams {
            interval_sec: 0.5,
            subdivision: 1,
            length_sec: 8,
            ..TrackParams::default()
        };
        build_track(&params, OutputFormat::Wav, TrackInputs::default(), |_| {})
            .unwrap()
            .bytes
    }

    #[test]
    fn names_follow_the_song_when_known() {
        let mut req = request();
        req.music = Some(MusicSource::Bytes(music_bytes()));
        req.music_name = Some("Queen - We Will Rock You.mp3".into());

        // 화면에서 확인한 값만 쓰는 경우: 적힌 것이 없으면 원래 파일 이름.
        let output = run(&req, |_, _| {}).unwrap();
        assert_eq!(output.file_name, "[1초] Queen - We Will Rock You.wav");
        assert!(output.song.is_empty());

        req.song = SongInfo::new(None, Some("직접 적은 곡"));
        let output = run(&req, |_, _| {}).unwrap();
        assert_eq!(output.file_name, "[1초] 직접 적은 곡.wav");

        // 찾아보는 경우: 파일 이름에서 가수와 곡을 나눈다. 사용자가 적은 가수는 그대로 둔다.
        req.song = SongInfo::new(Some("퀸"), None);
        req.song_lookup = SongLookup::Detect;
        let output = run(&req, |_, _| {}).unwrap();
        assert_eq!(output.file_name, "[1초] 퀸 - We Will Rock You.wav");
        assert_eq!(output.song.title.as_deref(), Some("We Will Rock You"));
        assert!(output.notices.is_empty(), "{:?}", output.notices);

        req.music_name = Some("track07.mp3".into());
        let output = run(&req, |_, _| {}).unwrap();
        assert_eq!(output.file_name, "[1초] track07.wav");
    }

    #[test]
    fn parses_music_source() {
        assert_eq!(
            MusicSource::parse(" https://example.com/a.mp3 "),
            MusicSource::Url("https://example.com/a.mp3".into())
        );
        assert_eq!(
            MusicSource::parse("songs/a.mp3"),
            MusicSource::Path(PathBuf::from("songs/a.mp3"))
        );
    }

    #[test]
    fn makes_beeps_without_any_inputs() {
        let mut stages = Vec::new();
        let output = run(&request(), |percent, stage| {
            stages.push((percent, stage.to_owned()))
        })
        .unwrap();
        assert_eq!(output.bytes.len(), 44 + 44_100 * 2);
        assert_eq!(output.file_name, "[1초] beep.wav");
        assert!(output.notices.is_empty());
        assert_eq!(stages.last().unwrap().0, 100);
    }

    #[test]
    fn empty_intro_template_means_no_intro() {
        let mut req = request();
        req.intro = Some(IntroRequest {
            template: "   ".into(),
            voice: None,
        });
        let output = run(&req, |_, _| {}).unwrap();
        assert_eq!(output.intro_seconds, 0.0);
        assert!(output.notices.is_empty());
    }

    #[test]
    fn unknown_voice_is_reported_but_track_is_still_made() {
        let mut req = request();
        req.intro = Some(IntroRequest {
            template: "hello".into(),
            voice: Some("no-such-voice-xyz".into()),
        });
        let output = run(&req, |_, _| {}).unwrap();
        assert_eq!(output.intro_seconds, 0.0);
        assert_eq!(output.notices.len(), 1, "{:?}", output.notices);
    }

    #[test]
    fn invalid_params_fail_before_fetching() {
        let mut req = request();
        req.params.subdivision = 0;
        // 주소가 잘못됐어도 설정값 오류가 먼저 나와야 한다.
        req.music = Some(MusicSource::Url("https://www.youtube.com/watch?v=x".into()));
        assert!(matches!(run(&req, |_, _| {}), Err(JobError::Input(_))));
    }

    #[test]
    fn missing_file_and_streaming_url_are_fetch_errors() {
        let mut req = request();
        req.music = Some(MusicSource::Path("no/such/file.mp3".into()));
        assert!(matches!(run(&req, |_, _| {}), Err(JobError::Fetch(_))));
        req.music = Some(MusicSource::Url("https://youtu.be/x".into()));
        assert!(matches!(run(&req, |_, _| {}), Err(JobError::Fetch(_))));
    }
}
