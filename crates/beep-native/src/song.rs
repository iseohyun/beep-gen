//! 배경 음악의 가수와 곡 제목 알아내기.
//!
//! 순서: (1) 음악 파일의 태그 → (2) `가수 - 곡` 꼴의 파일 이름 → (3) 로컬 AI.
//! (1)(2)는 코어가 하고(웹과 같다), (3)만 로컬에서 덧붙인다. 그래도 모르면 지어내지 않고
//! 빈 값을 돌려준다. 그때는 사용자가 직접 적거나, 원래 파일 이름이 그대로 쓰인다.

use beep_core::{SongInfo, SongSource};
use serde::Serialize;

use crate::ai;

/// 가수와 곡을 알아낸 곳.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentifiedBy {
    /// 음악 파일에 적힌 태그.
    Tags,
    /// `가수 - 곡` 꼴의 파일 이름.
    FileName,
    /// 로컬 AI 의 추정. 틀릴 수 있다.
    Ai,
}

/// 알아낸 결과.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Identified {
    #[serde(flatten)]
    pub song: SongInfo,
    /// 알아낸 곳. 알아내지 못했으면 `None`.
    pub by: Option<IdentifiedBy>,
    /// 로컬 AI 가 답했을 때 그 모델의 이름.
    pub model: Option<String>,
    /// 로컬 AI 에 물었지만 실패했을 때의 이유.
    pub problem: Option<String>,
}

/// 음악 파일의 내용과 이름에서 가수와 곡을 알아낸다.
///
/// `ask_ai` 가 참이면, 태그와 파일 이름으로 알아내지 못했을 때 로컬 AI 에 파일 이름을 묻는다.
pub fn identify(music: Option<&[u8]>, source_name: Option<&str>, ask_ai: bool) -> Identified {
    let guess = beep_core::identify_song(music, source_name);
    if let Some(source) = guess.source {
        return Identified {
            song: guess.song,
            by: Some(match source {
                SongSource::Tags => IdentifiedBy::Tags,
                SongSource::FileName => IdentifiedBy::FileName,
            }),
            ..Identified::default()
        };
    }
    let Some(name) = source_name.filter(|_| ask_ai) else {
        return Identified::default();
    };
    match ai::identify_song(name) {
        Ok(Some(answer)) => Identified {
            song: answer.song,
            by: Some(IdentifiedBy::Ai),
            model: Some(answer.model),
            problem: None,
        },
        Ok(None) => Identified::default(),
        Err(e) => Identified {
            problem: Some(e.to_string()),
            ..Identified::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_formed_names_never_reach_the_ai() {
        let found = identify(None, Some("Queen - Bohemian Rhapsody.mp3"), true);
        assert_eq!(found.by, Some(IdentifiedBy::FileName));
        assert_eq!(
            found.song,
            SongInfo::new(Some("Queen"), Some("Bohemian Rhapsody"))
        );
        assert_eq!(found.model, None);
    }

    #[test]
    fn nothing_is_invented_without_ai() {
        assert_eq!(
            identify(None, Some("track07.mp3"), false),
            Identified::default()
        );
        assert_eq!(identify(None, None, true), Identified::default());
    }

    #[test]
    fn serializes_flat_for_the_ui() {
        let found = identify(None, Some("A - B.mp3"), false);
        let json = serde_json::to_value(&found).unwrap();
        assert_eq!(json["artist"], "A");
        assert_eq!(json["title"], "B");
        assert_eq!(json["by"], "file-name");
    }
}
