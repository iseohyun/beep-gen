//! 결과 파일의 이름 짓기.
//!
//! 규칙: `[{주기}초] {가수} - {곡}.{확장자}`
//!
//! - 배경 음악이 없으면 `[1.4초] beep.mp3`
//! - 가수와 곡을 알면 `[1.4초] 가수 - 곡.mp3`, 곡만 알면 `[1.4초] 곡.mp3`
//! - 곡을 모르면 원래 음악 파일의 이름을 쓴다: `[1.4초] 원래이름.mp3`
//!
//! 가수와 곡은 (1) 음악 파일에 적힌 태그 (2) `가수 - 곡` 꼴의 파일 이름에서 찾는다.
//! 그래도 모르면 로컬 어댑터가 로컬 AI 에 묻거나 사용자가 직접 적는다(코어 밖의 일).

use serde::{Deserialize, Serialize};

use crate::decode::read_song_tags;
use crate::encode::OutputFormat;
use crate::params::TrackParams;

/// 배경 음악이 없을 때 쓰는 이름.
const BEEP_ONLY_NAME: &str = "beep";
/// 곡도 원래 파일 이름도 모를 때 쓰는 이름.
const UNKNOWN_MUSIC_NAME: &str = "music";
/// 이름 한 토막의 최대 길이(글자 수). 운영체제의 파일 이름 한계를 넘지 않게 한다.
const MAX_PART_CHARS: usize = 80;

/// 가수와 곡 제목. 모르는 쪽은 비워 둔다.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongInfo {
    pub artist: Option<String>,
    pub title: Option<String>,
}

impl SongInfo {
    pub fn new(artist: Option<&str>, title: Option<&str>) -> Self {
        Self {
            artist: artist.map(str::to_owned),
            title: title.map(str::to_owned),
        }
        .cleaned()
    }

    /// 앞뒤 공백을 떼고, 비어 있는 값은 "모름" 으로 바꾼다.
    pub fn cleaned(self) -> Self {
        let clean =
            |value: Option<String>| value.map(|v| collapse_spaces(&v)).filter(|v| !v.is_empty());
        Self {
            artist: clean(self.artist),
            title: clean(self.title),
        }
    }

    /// 가수와 곡을 모두 아는지.
    pub fn is_complete(&self) -> bool {
        self.artist.is_some() && self.title.is_some()
    }

    /// 아무것도 모르는지.
    pub fn is_empty(&self) -> bool {
        self.artist.is_none() && self.title.is_none()
    }
}

/// 가수와 곡을 어디서 알아냈는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SongSource {
    /// 음악 파일에 적힌 태그.
    Tags,
    /// `가수 - 곡` 꼴의 파일 이름.
    FileName,
}

/// 가수와 곡에 대한 추정.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SongGuess {
    pub song: SongInfo,
    /// 알아낸 곳. 알아내지 못했으면 `None`.
    pub source: Option<SongSource>,
}

/// 결과 파일의 이름을 지을 때 쓰는, 배경 음악에 대해 아는 것.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MusicName {
    /// 가수와 곡. 모르면 비워 둔다.
    pub song: SongInfo,
    /// 원래 음악 파일의 이름(경로나 확장자가 붙어 있어도 된다). 모르면 `None`.
    pub source_name: Option<String>,
}

/// 음악 파일의 내용과 이름에서 가수와 곡을 찾는다.
///
/// 태그에 곡 제목이 있으면 태그를 믿고, 없으면 파일 이름이 `가수 - 곡` 꼴일 때만 그것을 쓴다.
/// 어느 쪽도 아니면 지어내지 않고 빈 값을 돌려준다.
pub fn identify_song(music: Option<&[u8]>, source_name: Option<&str>) -> SongGuess {
    let tags = music.map(read_song_tags).unwrap_or_default();
    let from_name = source_name.map(song_from_file_name).unwrap_or_default();
    if tags.title.is_some() {
        return SongGuess {
            song: SongInfo {
                artist: tags.artist.or(from_name.artist),
                title: tags.title,
            },
            source: Some(SongSource::Tags),
        };
    }
    if from_name.is_complete() {
        return SongGuess {
            song: from_name,
            source: Some(SongSource::FileName),
        };
    }
    SongGuess::default()
}

/// 결과 파일의 이름. `music` 이 `None` 이면 비프만 있는 음원이다.
pub fn output_file_name(
    params: &TrackParams,
    format: OutputFormat,
    music: Option<&MusicName>,
) -> String {
    let body = match music {
        None => BEEP_ONLY_NAME.to_owned(),
        Some(music) => {
            let song = music.song.clone().cleaned();
            let title = song.title.as_deref().and_then(safe_part);
            let artist = song.artist.as_deref().and_then(safe_part);
            match (artist, title) {
                (Some(artist), Some(title)) => format!("{artist} - {title}"),
                (None, Some(title)) => title,
                // 곡을 모르면 원래 파일 이름을 그대로 쓴다.
                (_, None) => music
                    .source_name
                    .as_deref()
                    .and_then(|name| safe_part(file_stem(name)))
                    .unwrap_or_else(|| UNKNOWN_MUSIC_NAME.to_owned()),
            }
        }
    };
    format!(
        "[{}초] {}.{}",
        params.interval_label(),
        body,
        format.extension()
    )
}

/// 경로에서 폴더와 확장자를 뗀 이름.
pub fn file_stem(name: &str) -> &str {
    let name = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match name.rsplit_once('.') {
        // 짧은 영숫자 꼬리만 확장자로 본다("Mr. Blue Sky" 의 뒤쪽을 자르지 않게).
        Some((stem, extension))
            if !stem.is_empty()
                && (1..=5).contains(&extension.len())
                && extension.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            stem
        }
        _ => name,
    }
}

/// 파일 이름이 `가수 - 곡` 꼴이면 둘로 나눈다. 아니면 빈 값.
pub fn song_from_file_name(name: &str) -> SongInfo {
    let mut text = file_stem(name).to_owned();
    // 공백 대신 밑줄을 쓴 이름("Artist_-_Title")은 밑줄을 공백으로 본다.
    if !text.contains(' ') {
        text = text.replace('_', " ");
    }
    let text = collapse_spaces(&strip_noise_brackets(&text));
    let text = strip_track_number(&text);

    for separator in [" - ", " – ", " — "] {
        if let Some((artist, title)) = text.split_once(separator) {
            let song = SongInfo::new(Some(artist), Some(title));
            // 내려받기 사이트 이름이나 꼬리표가 섞인 이름은 깔끔하게 나뉜 것이 아니다.
            let lower_title = title.to_lowercase();
            let untidy =
                looks_like_site(artist) || NOISE.iter().any(|word| has_word(&lower_title, word));
            if song.is_complete() && !untidy {
                return song;
            }
        }
    }
    SongInfo::default()
}

/// 곡과 무관한 꼬리표에 쓰이는 낱말.
const NOISE: [&str; 19] = [
    "official",
    "video",
    "audio",
    "lyric",
    "lyrics",
    "m/v",
    "mv",
    "가사",
    "뮤직비디오",
    "음원",
    "고음질",
    "hd",
    "hq",
    "4k",
    "1080p",
    "720p",
    "320k",
    "mp3",
    "full ver",
];

/// 웹 사이트 이름처럼 보이는지(예: `y2mate.com`).
fn looks_like_site(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    lower.starts_with("www.")
        || [".com", ".net", ".org", ".io", ".kr", ".cc", ".to", ".me"]
            .iter()
            .any(|ending| lower.ends_with(ending))
}

/// 괄호 안이 곡과 무관한 꼬리표(Official Video, MV, 가사 등)이면 괄호째 지운다.
fn strip_noise_brackets(text: &str) -> String {
    const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('【', '】')];

    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(|c| PAIRS.iter().any(|(open, _)| *open == c)) {
        let open = rest[start..].chars().next().unwrap_or('(');
        let close = PAIRS
            .iter()
            .find(|(o, _)| *o == open)
            .map_or(')', |(_, c)| *c);
        let inner_start = start + open.len_utf8();
        let Some(length) = rest[inner_start..].find(close) else {
            break;
        };
        let inner = rest[inner_start..inner_start + length].to_lowercase();
        out.push_str(&rest[..start]);
        if !NOISE.iter().any(|word| has_word(&inner, word)) {
            out.push_str(&rest[start..inner_start + length + close.len_utf8()]);
        } else {
            out.push(' ');
        }
        rest = &rest[inner_start + length + close.len_utf8()..];
    }
    out.push_str(rest);
    out
}

/// `text` 에 `word` 가 낱말로 들어 있는지("mv" 가 "improve" 에 걸리지 않게).
fn has_word(text: &str, word: &str) -> bool {
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        // 한글 꼬리표는 조사가 붙을 수 있으므로 앞뒤를 따지지 않는다.
        !word.is_ascii()
            || (!before.is_some_and(|c| c.is_ascii_alphanumeric())
                && !after.is_some_and(|c| c.is_ascii_alphabetic()))
    })
}

/// 맨 앞의 트랙 번호("01. ", "3) ", "01 - 가수 - 곡" 의 "01 - ")를 뗀다.
fn strip_track_number(text: &str) -> &str {
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if !(1..=3).contains(&digits) {
        return text;
    }
    let rest = &text[digits..];
    if let Some(after) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
        return after.trim_start();
    }
    // "10cm - 곡" 처럼 숫자가 가수 이름일 수 있으므로, 구분자가 두 번 나올 때만 번호로 본다.
    if let Some(after) = rest.strip_prefix(" - ") {
        if after.contains(" - ") {
            return after;
        }
    }
    text
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 파일 이름에 쓸 수 없는 글자를 바꾸고 길이를 줄인다. 남는 것이 없으면 `None`.
fn safe_part(text: &str) -> Option<String> {
    let replaced: String = text
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let shortened: String = collapse_spaces(&replaced)
        .chars()
        .take(MAX_PART_CHARS)
        .collect();
    // Windows 는 끝이 점이나 공백인 이름을 받지 않는다.
    let trimmed = shortened.trim().trim_end_matches(['.', ' ']);
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(params: &TrackParams, music: Option<&MusicName>) -> String {
        output_file_name(params, OutputFormat::Mp3, music)
    }

    fn music(artist: Option<&str>, title: Option<&str>, source: Option<&str>) -> MusicName {
        MusicName {
            song: SongInfo::new(artist, title),
            source_name: source.map(str::to_owned),
        }
    }

    #[test]
    fn beeps_only_use_the_fixed_name() {
        let mut params = TrackParams::default();
        assert_eq!(name(&params, None), "[1초] beep.mp3");
        params.interval_sec = 1.4;
        assert_eq!(name(&params, None), "[1.4초] beep.mp3");
        assert_eq!(
            output_file_name(&params, OutputFormat::Wav, None),
            "[1.4초] beep.wav"
        );
    }

    #[test]
    fn music_names_follow_what_is_known() {
        let params = TrackParams {
            interval_sec: 1.4,
            ..TrackParams::default()
        };
        let cases = [
            (
                music(Some("아이유"), Some("Blueming"), Some("x.mp3")),
                "[1.4초] 아이유 - Blueming.mp3",
            ),
            // 곡만 알면 곡만.
            (
                music(None, Some("Blueming"), Some("x.mp3")),
                "[1.4초] Blueming.mp3",
            ),
            // 곡을 모르면 원래 파일 이름(가수만 아는 경우도).
            (
                music(Some("아이유"), None, Some("C:\\음악\\track 07.flac")),
                "[1.4초] track 07.mp3",
            ),
            (
                music(None, None, Some("songs/my.song.v2.mp3")),
                "[1.4초] my.song.v2.mp3",
            ),
            (music(None, None, None), "[1.4초] music.mp3"),
            (music(Some("  "), Some(" "), Some("a.mp3")), "[1.4초] a.mp3"),
        ];
        for (music, expected) in cases {
            assert_eq!(name(&params, Some(&music)), expected, "{music:?}");
        }
    }

    #[test]
    fn unsafe_characters_are_replaced() {
        let params = TrackParams::default();
        let m = music(Some("AC/DC"), Some("Who Made Who? <Live>..."), None);
        assert_eq!(
            name(&params, Some(&m)),
            "[1초] AC_DC - Who Made Who_ _Live_.mp3"
        );
        let long = "가".repeat(300);
        let m = music(None, Some(&long), None);
        assert_eq!(
            name(&params, Some(&m)).chars().count(),
            "[1초] .mp3".len() - 2 + 80
        );
        // 바꾸고 나면 남는 것이 없는 제목은 모르는 것으로 본다.
        let m = music(None, Some("..."), Some("orig.mp3"));
        assert_eq!(name(&params, Some(&m)), "[1초] orig.mp3");
    }

    #[test]
    fn file_stem_drops_folders_and_extension() {
        assert_eq!(file_stem("a/b\\c.mp3"), "c");
        assert_eq!(file_stem("Mr. Blue Sky"), "Mr. Blue Sky");
        assert_eq!(file_stem(".hidden"), ".hidden");
        assert_eq!(file_stem("noext"), "noext");
    }

    #[test]
    fn splits_well_formed_file_names() {
        let cases = [
            ("아이유 - Blueming.mp3", "아이유", "Blueming"),
            ("IU_-_Blueming.mp3", "IU", "Blueming"),
            (
                "01. Queen - Bohemian Rhapsody.flac",
                "Queen",
                "Bohemian Rhapsody",
            ),
            (
                "03 - Queen - Don't Stop Me Now.mp3",
                "Queen",
                "Don't Stop Me Now",
            ),
            ("10cm - 봄이 좋냐.mp3", "10cm", "봄이 좋냐"),
            (
                "[MV] NewJeans - Hype Boy (Official Video) [4K].m4a",
                "NewJeans",
                "Hype Boy",
            ),
            (
                "Daft Punk – One More Time (Remastered).mp3",
                "Daft Punk",
                "One More Time (Remastered)",
            ),
            ("A - B - C.mp3", "A", "B - C"),
        ];
        for (file, artist, title) in cases {
            assert_eq!(
                song_from_file_name(file),
                SongInfo::new(Some(artist), Some(title)),
                "{file}"
            );
        }
    }

    #[test]
    fn unclear_file_names_are_not_guessed() {
        for file in [
            "Blueming.mp3",
            "track07.mp3",
            "아이유-블루밍.mp3",
            " - .mp3",
            "",
            "y2mate.com - Queen Bohemian Rhapsody.mp3",
            "Queen - Bohemian Rhapsody Official Video.mp3",
        ] {
            assert!(song_from_file_name(file).is_empty(), "{file}");
        }
    }

    /// 가수·곡 태그(ID3v2.4)가 붙은 짧은 MP3 를 만든다.
    fn tagged_mp3(artist: Option<&str>, title: Option<&str>) -> Vec<u8> {
        fn syncsafe(n: usize) -> [u8; 4] {
            [
                (n >> 21) as u8 & 0x7f,
                (n >> 14) as u8 & 0x7f,
                (n >> 7) as u8 & 0x7f,
                n as u8 & 0x7f,
            ]
        }
        let mut frames = Vec::new();
        for (id, text) in [("TPE1", artist), ("TIT2", title)] {
            let Some(text) = text else { continue };
            frames.extend_from_slice(id.as_bytes());
            frames.extend_from_slice(&syncsafe(text.len() + 1));
            frames.extend_from_slice(&[0, 0, 3]); // 플래그 2바이트 + UTF-8 표시
            frames.extend_from_slice(text.as_bytes());
        }
        let mut file = b"ID3\x04\x00\x00".to_vec();
        file.extend_from_slice(&syncsafe(frames.len()));
        file.extend_from_slice(&frames);

        let params = TrackParams {
            length_sec: 1,
            ..TrackParams::default()
        };
        let audio = crate::build_track(&params, OutputFormat::Mp3, Default::default(), |_| {})
            .unwrap()
            .bytes;
        file.extend_from_slice(&audio);
        file
    }

    #[test]
    fn tags_win_over_the_file_name() {
        let file = tagged_mp3(Some("아이유"), Some("Blueming"));
        let guess = identify_song(Some(&file), Some("Wrong - Name.mp3"));
        assert_eq!(guess.source, Some(SongSource::Tags));
        assert_eq!(guess.song, SongInfo::new(Some("아이유"), Some("Blueming")));
        // 태그가 붙어 있어도 소리는 그대로 읽힌다.
        assert!(crate::decode::decode_audio(&file).is_ok());
    }

    #[test]
    fn title_only_tags_take_the_artist_from_the_file_name() {
        let file = tagged_mp3(None, Some("Blueming"));
        let guess = identify_song(Some(&file), Some("IU - whatever.mp3"));
        assert_eq!(guess.song, SongInfo::new(Some("IU"), Some("Blueming")));
        let guess = identify_song(Some(&file), Some("track07.mp3"));
        assert_eq!(guess.song, SongInfo::new(None, Some("Blueming")));
    }

    #[test]
    fn artist_only_tags_are_not_enough() {
        let file = tagged_mp3(Some("아이유"), None);
        assert_eq!(
            identify_song(Some(&file), Some("track07.mp3")),
            SongGuess::default()
        );
    }

    #[test]
    fn identify_without_tags_falls_back_to_the_file_name() {
        let guess = identify_song(None, Some("Queen - Bohemian Rhapsody.mp3"));
        assert_eq!(guess.source, Some(SongSource::FileName));
        assert_eq!(guess.song.artist.as_deref(), Some("Queen"));

        // 읽을 수 없는 내용이어도 실패하지 않는다.
        let guess = identify_song(Some(&[1, 2, 3]), Some("unknown.mp3"));
        assert_eq!(guess, SongGuess::default());
    }
}
