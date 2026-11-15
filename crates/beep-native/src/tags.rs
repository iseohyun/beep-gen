//! 음악 파일에 곡 정보 적어 두기.
//!
//! 가수와 곡을 한 번 알아냈으면(직접 적었든, AI 가 찾아 줬든) 음악 파일 자체에 적어 둔다.
//! 그러면 다음에 그 파일을 불러올 때 다시 찾을 필요가 없다. 파일 이름도 `가수 - 곡` 으로 바꾼다.
//!
//! 적는 항목과 자리(MP3 의 ID3 기준, 다른 형식은 그에 해당하는 자리):
//! - 곡 제목(TIT2), 가수(TPE1), 작곡자(TCOM)
//! - 원래 파일 이름(TOFN): 이름을 바꾸기 전의 이름. 이미 적혀 있으면 덮어쓰지 않는다.
//! - 출처 링크와 AI 활용 여부: 설명(Comment)에 한 줄씩 적는다.
//!   `SOURCE_URL=https://...`, `AI_GENERATED=Yes`(또는 `No`)
//! - 분석한 박자 계열과 템포: 설명에 `METER=3`(2박 계열은 2), `BPM=65.3` 한 줄씩.
//!   템포는 표준 자리(TBPM)에도 정수로 적는다.
//!   (형식마다 따로 둘 자리가 마땅치 않고, 설명은 탐색기나 재생기에서도 보인다.)
//!
//! 원본을 직접 고치는 일이라 조심한다: 사본에 먼저 적고, 그 사본이 제대로 읽히는지 확인한 뒤에
//! 원본과 바꾼다. 같은 이름의 다른 파일이 이미 있으면 덮어쓰지 않고 그만둔다.
//! (정책: `.agents/rules/project_architecture.md` 4.9절)

use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::{Accessor, ItemKey, Tag};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 설명(Comment)에 적는 줄의 머리.
const AI_LINE: &str = "AI_GENERATED=";
const URL_LINE: &str = "SOURCE_URL=";
const METER_LINE: &str = "METER=";
const BPM_LINE: &str = "BPM=";

/// 설명에서 `head` 로 시작하는 줄의 값을 찾는다.
fn comment_value(tag: &Tag, head: &str) -> Option<String> {
    tag.get_string(ItemKey::Comment)?
        .lines()
        .find_map(|line| line.trim().strip_prefix(head))
        .map(|value| value.trim().to_owned())
}

/// 설명의 다른 내용은 남기고, `head` 로 시작하는 줄만 `value` 로 바꾼다.
fn set_comment_value(tag: &mut Tag, head: &str, value: &str) {
    let mut lines: Vec<String> = tag
        .get_string(ItemKey::Comment)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().starts_with(head) && !line.trim().is_empty())
        .map(str::to_owned)
        .collect();
    lines.push(format!("{head}{value}"));
    tag.insert_text(
        ItemKey::Comment,
        lines.join(
            "
",
        ),
    );
}

/// 음악 파일에 적어 두는 곡 정보. 비어 있는 항목은 "모름" 이다.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SongTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub composer: Option<String>,
    /// 곡을 받은 곳의 주소(예: 동영상 주소).
    pub source_url: Option<String>,
    /// AI 로 만든 곡인지.
    pub ai_generated: Option<bool>,
    /// 이름을 바꾸기 전의 파일 이름.
    pub original_name: Option<String>,
    /// 박자 계열: 2 = 2박 계열(2/4, 4/4), 3 = 3박 계열(3/4, 6/8).
    pub meter: Option<u32>,
    /// 템포(BPM). 3박 계열은 강세(맥박 세 개)의 빠르기다.
    pub bpm: Option<f64>,
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|v| !v.is_empty())
}

impl SongTags {
    /// 앞뒤 공백을 떼고, 비어 있는 값은 "모름" 으로 바꾼다.
    pub fn cleaned(self) -> Self {
        Self {
            title: clean(self.title),
            artist: clean(self.artist),
            composer: clean(self.composer),
            source_url: clean(self.source_url),
            ai_generated: self.ai_generated,
            original_name: clean(self.original_name),
            meter: self.meter.filter(|meter| matches!(meter, 2 | 3)),
            bpm: self.bpm.filter(|bpm| bpm.is_finite() && *bpm > 0.0),
        }
    }

    fn from_tag(tag: &Tag) -> Self {
        let text = |key: ItemKey| tag.get_string(key).map(str::to_owned);
        let ai_generated =
            comment_value(tag, AI_LINE).map(|value| value.eq_ignore_ascii_case("yes"));
        Self {
            title: tag.title().map(|v| v.to_string()),
            artist: tag.artist().map(|v| v.to_string()),
            composer: text(ItemKey::Composer),
            source_url: comment_value(tag, URL_LINE),
            ai_generated,
            original_name: text(ItemKey::OriginalFileName),
            meter: comment_value(tag, METER_LINE).and_then(|value| value.parse().ok()),
            // 우리가 적은 값(소수 포함)을 먼저 보고, 없으면 표준 자리의 값을 쓴다.
            bpm: comment_value(tag, BPM_LINE)
                .or_else(|| text(ItemKey::Bpm))
                .or_else(|| text(ItemKey::IntegerBpm))
                .and_then(|value| value.trim().parse().ok()),
        }
        .cleaned()
    }

    /// 아는 항목만 태그에 적는다. 모르는 항목은 원래 값을 그대로 둔다.
    fn apply_to(&self, tag: &mut Tag) {
        if let Some(title) = &self.title {
            tag.set_title(title.clone());
        }
        if let Some(artist) = &self.artist {
            tag.set_artist(artist.clone());
        }
        for (key, value) in [
            (ItemKey::Composer, &self.composer),
            (ItemKey::OriginalFileName, &self.original_name),
        ] {
            if let Some(value) = value {
                tag.insert_text(key, value.clone());
            }
        }
        if let Some(url) = &self.source_url {
            set_comment_value(tag, URL_LINE, url);
        }
        if let Some(ai_generated) = self.ai_generated {
            set_comment_value(tag, AI_LINE, if ai_generated { "Yes" } else { "No" });
        }
        if let Some(meter) = self.meter {
            set_comment_value(tag, METER_LINE, &meter.to_string());
        }
        if let Some(bpm) = self.bpm {
            set_comment_value(tag, BPM_LINE, &format!("{bpm:.1}"));
            tag.insert_text(ItemKey::IntegerBpm, format!("{bpm:.0}"));
        }
    }
}

/// 음악 파일에 적혀 있는 곡 정보를 읽는다. 태그가 없으면 빈 값.
pub fn read(path: &Path) -> Result<SongTags> {
    let file = lofty::read_from_path(path)
        .map_err(|e| Error(format!("{} 의 정보를 읽지 못했습니다: {e}", path.display())))?;
    Ok(file
        .primary_tag()
        .or_else(|| file.first_tag())
        .map(SongTags::from_tag)
        .unwrap_or_default())
}

/// 저장한 결과.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Saved {
    /// 저장한 뒤의 파일 경로(이름을 바꿨으면 새 경로).
    pub path: PathBuf,
    /// 파일에 적힌 최종 정보.
    pub tags: SongTags,
    /// 파일 이름을 바꿨는지.
    pub renamed: bool,
}

/// 곡 정보에 맞는 파일 이름(`가수 - 곡.확장자`, 가수를 모르면 `곡.확장자`). 곡을 모르면 `None`.
pub fn tidy_file_name(tags: &SongTags, extension: &str) -> Option<String> {
    let title = tags.title.as_deref()?;
    let label = match tags.artist.as_deref() {
        Some(artist) => format!("{artist} - {title}"),
        None => title.to_owned(),
    };
    // 파일 이름에 쓸 수 없는 글자를 바꾼다.
    let safe: String = label
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let safe = safe.split_whitespace().collect::<Vec<_>>().join(" ");
    let safe = safe.trim_end_matches(['.', ' ']);
    (!safe.is_empty()).then(|| format!("{safe}.{extension}"))
}

/// 음악 파일에 곡 정보를 적고, `rename` 이면 이름을 `가수 - 곡` 으로 바꾼다.
///
/// `tags` 에서 비어 있는 항목은 파일에 이미 적힌 값을 그대로 둔다. 원래 파일 이름은
/// 이름을 처음 바꿀 때 한 번만 적는다.
pub fn save(path: &Path, tags: &SongTags, rename: bool) -> Result<Saved> {
    let fail = |what: &str, e: &dyn std::fmt::Display| Error(format!("{what}: {e}"));
    let folder = path
        .parent()
        .ok_or_else(|| Error(format!("폴더를 알 수 없습니다: {}", path.display())))?;
    let current_name = path
        .file_name()
        .ok_or_else(|| Error(format!("파일이 아닙니다: {}", path.display())))?
        .to_string_lossy()
        .into_owned();
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut file = lofty::read_from_path(path)
        .map_err(|e| fail(&format!("{current_name} 을(를) 열지 못했습니다"), &e))?;
    let tag_type = file.primary_tag_type();
    if file.tag(tag_type).is_none() {
        file.insert_tag(Tag::new(tag_type));
    }
    let tag = file
        .tag_mut(tag_type)
        .ok_or_else(|| Error("태그를 만들지 못했습니다".into()))?;

    // 새 이름을 미리 정한다: 적을 값과 이미 적힌 값을 합친 결과로 짓는다.
    let mut wanted = tags.clone().cleaned();
    let existing = SongTags::from_tag(tag);
    let merged = SongTags {
        title: wanted.title.clone().or(existing.title.clone()),
        artist: wanted.artist.clone().or(existing.artist.clone()),
        ..SongTags::default()
    };
    let new_name = if rename {
        tidy_file_name(&merged, &extension).unwrap_or_else(|| current_name.clone())
    } else {
        current_name.clone()
    };
    let renamed = new_name != current_name;
    let target = folder.join(&new_name);
    // 대소문자만 다른 이름은 Windows 에서 같은 파일을 가리킨다. 그때는 덮어쓰는 것이 아니다.
    let same_file = renamed
        && target.exists()
        && std::fs::canonicalize(&target).ok() == std::fs::canonicalize(path).ok();
    if renamed && target.exists() && !same_file {
        return Err(Error(format!(
            "같은 이름의 파일이 이미 있어 그만뒀습니다: {}",
            target.display()
        )));
    }
    // 원래 이름은 처음 한 번만 남긴다(두 번째로 바꿀 때 첫 이름이 지워지지 않게).
    wanted.original_name = match existing.original_name {
        Some(_) => None,
        None => wanted
            .original_name
            .or(renamed.then(|| current_name.clone())),
    };
    wanted.apply_to(tag);

    // 사본에 적고, 제대로 읽히는지 확인한 뒤 원본과 바꾼다.
    // 사본도 같은 확장자를 써야 형식을 알아본다.
    let temp = folder.join(format!(".beep-gen-tmp-{}.{extension}", std::process::id()));
    let cleanup = |e: Error| {
        let _ = std::fs::remove_file(&temp);
        e
    };
    std::fs::copy(path, &temp).map_err(|e| fail("사본을 만들지 못했습니다", &e))?;
    file.save_to_path(&temp, WriteOptions::default())
        .map_err(|e| cleanup(fail("정보를 적지 못했습니다", &e)))?;
    let written = std::fs::read(&temp).map_err(|e| cleanup(fail("사본을 읽지 못했습니다", &e)))?;
    // 소리가 그대로 읽히는지: 형식 판별과 태그 읽기가 되면 파일 구조가 멀쩡한 것이다.
    let check = beep_core::decode::read_song_tags(&written);
    let saved_tags = read(&temp).map_err(cleanup)?;
    if merged.title.is_some() && check.title != merged.title && saved_tags.title != merged.title {
        return Err(cleanup(Error(
            "적은 정보가 다시 읽히지 않아 원본을 그대로 뒀습니다".into(),
        )));
    }
    drop(written);
    if same_file {
        // 원본 자리에 바꿔 넣은 뒤 이름의 대소문자만 고친다.
        std::fs::rename(&temp, path).map_err(|e| cleanup(fail("원본과 바꾸지 못했습니다", &e)))?;
        std::fs::rename(path, &target).map_err(|e| fail("이름을 바꾸지 못했습니다", &e))?;
    } else {
        std::fs::rename(&temp, &target)
            .map_err(|e| cleanup(fail("원본과 바꾸지 못했습니다", &e)))?;
    }
    if renamed && !same_file {
        // 새 이름으로 저장이 끝났으므로 옛 이름의 파일을 지운다.
        std::fs::remove_file(path).map_err(|e| {
            fail(
                &format!(
                    "새 파일({new_name})은 만들었지만 옛 파일({current_name})을 지우지 못했습니다"
                ),
                &e,
            )
        })?;
    }
    Ok(Saved {
        path: target,
        tags: saved_tags,
        renamed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 태그가 없는 짧은 MP3 를 임시 폴더에 만든다.
    fn sample(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "beep-gen-tags-{}-{}",
            std::process::id(),
            name.len()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let params = beep_core::TrackParams {
            length_sec: 2,
            ..Default::default()
        };
        let bytes = beep_core::build_track(
            &params,
            beep_core::OutputFormat::Mp3,
            Default::default(),
            |_| {},
        )
        .unwrap()
        .bytes;
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn wanted() -> SongTags {
        SongTags {
            title: Some("Come a Little Closer".into()),
            artist: Some("Foyage".into()),
            composer: Some("Foyage".into()),
            source_url: Some("https://example.com/watch?v=abc".into()),
            ai_generated: Some(true),
            original_name: None,
            meter: Some(3),
            bpm: Some(65.3),
        }
    }

    #[test]
    fn saves_tags_and_renames_the_file() {
        let path = sample("TS 말포이 노래 Come a Little Closer.mp3");
        let before = std::fs::read(&path).unwrap();
        assert_eq!(read(&path).unwrap(), SongTags::default());

        let saved = save(&path, &wanted(), true).unwrap();
        assert!(saved.renamed);
        assert_eq!(
            saved.path.file_name().unwrap().to_string_lossy(),
            "Foyage - Come a Little Closer.mp3"
        );
        assert!(!path.exists(), "옛 이름의 파일은 없어야 한다");

        let tags = read(&saved.path).unwrap();
        assert_eq!(
            tags,
            SongTags {
                original_name: Some("TS 말포이 노래 Come a Little Closer.mp3".into()),
                ..wanted()
            }
        );
        // 코어(웹과 공용)도 같은 가수와 곡을 읽는다: 다음에 불러올 때 다시 찾지 않아도 된다.
        let bytes = std::fs::read(&saved.path).unwrap();
        let guess = beep_core::identify_song(Some(&bytes), Some("아무 이름.mp3"));
        assert_eq!(guess.song.artist.as_deref(), Some("Foyage"));
        assert_eq!(guess.song.title.as_deref(), Some("Come a Little Closer"));
        // 소리는 그대로다(태그만 앞에 붙었다).
        assert!(bytes.ends_with(&before[before.len() - 4000..]));
        assert!(beep_core::decode::decode_audio(&bytes).is_ok());
        // 임시 파일이 남지 않는다.
        let leftovers = std::fs::read_dir(saved.path.parent().unwrap())
            .unwrap()
            .count();
        assert_eq!(leftovers, 1);
    }

    #[test]
    fn a_second_save_keeps_the_first_original_name_and_other_fields() {
        let path = sample("raw download.mp3");
        let first = save(&path, &wanted(), true).unwrap();
        // 곡 제목만 고친다. 나머지는 그대로 남아야 한다.
        let fix = SongTags {
            title: Some("Come A Little Closer".into()),
            ai_generated: Some(false),
            ..SongTags::default()
        };
        let second = save(&first.path, &fix, true).unwrap();
        assert_eq!(
            second.path.file_name().unwrap().to_string_lossy(),
            "Foyage - Come A Little Closer.mp3"
        );
        let tags = read(&second.path).unwrap();
        assert_eq!(tags.original_name.as_deref(), Some("raw download.mp3"));
        assert_eq!(tags.artist.as_deref(), Some("Foyage"));
        assert_eq!(tags.source_url, wanted().source_url);
        assert_eq!(tags.ai_generated, Some(false));
    }

    #[test]
    fn an_existing_file_is_never_overwritten() {
        let path = sample("first.mp3");
        let other = path.with_file_name("Foyage - Come a Little Closer.mp3");
        std::fs::write(&other, b"someone else's file").unwrap();
        let error = save(&path, &wanted(), true).unwrap_err();
        assert!(error.0.contains("이미 있어"), "{error}");
        assert_eq!(std::fs::read(&other).unwrap(), b"someone else's file");
        // 원본도 손대지 않았다.
        assert_eq!(read(&path).unwrap(), SongTags::default());
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            2
        );
    }

    #[test]
    fn saving_without_renaming_keeps_the_name() {
        let path = sample("keep name.mp3");
        let saved = save(&path, &wanted(), false).unwrap();
        assert!(!saved.renamed);
        assert_eq!(saved.path, path);
        assert_eq!(read(&path).unwrap().original_name, None);
        assert_eq!(read(&path).unwrap().title, wanted().title);
    }

    #[test]
    fn tidy_names_follow_what_is_known() {
        let tags = SongTags {
            title: Some("A/B: C?".into()),
            ..SongTags::default()
        };
        assert_eq!(tidy_file_name(&tags, "mp3").as_deref(), Some("A_B_ C_.mp3"));
        assert_eq!(tidy_file_name(&SongTags::default(), "mp3"), None);
    }
}
