//! 음성 합성(TTS): 문구를 음성 파일로 만든다.
//!
//! 지금은 Windows 내장 음성만 지원한다(인터넷 불필요). 다른 엔진을 추가할 때는
//! 이 모듈의 세 함수(`is_supported`, `list_voices`, `synthesize`)가 같은 모양을 유지하면
//! CLI·앱·MCP 를 고칠 필요가 없다.

use serde::Serialize;

use crate::Result;

/// 고를 수 있는 음성 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Voice {
    /// 음성을 가리키는 식별자. `synthesize` 에 그대로 넘긴다.
    pub id: String,
    /// 화면에 보일 이름. 예: "Microsoft Heami"
    pub name: String,
    /// 언어 코드. 예: "ko-KR"
    pub language: String,
}

/// 이 환경에서 음성 합성을 쓸 수 있는지.
pub fn is_supported() -> bool {
    cfg!(windows)
}

/// 설치된 음성 목록.
pub fn list_voices() -> Result<Vec<Voice>> {
    imp::list_voices()
}

/// 문구를 읽은 음성을 WAV 파일 바이트로 돌려준다.
///
/// `voice_id` 를 생략하면 문구에 한글이 있을 때 한국어 음성을, 없으면 시스템 기본 음성을 쓴다.
pub fn synthesize(text: &str, voice_id: Option<&str>) -> Result<Vec<u8>> {
    let text = text.trim();
    if text.is_empty() {
        return Err(crate::Error("읽을 문구가 비어 있습니다.".into()));
    }
    imp::synthesize(text, voice_id)
}

/// 문구에 한글이 들어 있는지.
#[cfg_attr(not(windows), allow(dead_code))]
fn contains_hangul(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, '\u{AC00}'..='\u{D7A3}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}'))
}

#[cfg(windows)]
mod imp {
    use windows::Media::SpeechSynthesis::{SpeechSynthesizer, VoiceInformation};
    use windows::Storage::Streams::DataReader;
    use windows::core::{HSTRING, Interface};
    use windows_future::IAsyncOperation;

    use super::{Voice, contains_hangul};
    use crate::{Error, Result};

    fn fail(action: &str) -> impl Fn(windows::core::Error) -> Error + '_ {
        move |e| Error(format!("{action}: {}", e.message()))
    }

    fn all_voices() -> Result<Vec<VoiceInformation>> {
        let voices = SpeechSynthesizer::AllVoices().map_err(fail("음성 목록을 읽지 못했습니다"))?;
        Ok(voices.into_iter().collect())
    }

    fn describe(voice: &VoiceInformation) -> Result<Voice> {
        let read = fail("음성 정보를 읽지 못했습니다");
        Ok(Voice {
            id: voice.Id().map_err(&read)?.to_string(),
            name: voice.DisplayName().map_err(&read)?.to_string(),
            language: voice.Language().map_err(&read)?.to_string(),
        })
    }

    pub fn list_voices() -> Result<Vec<Voice>> {
        all_voices()?.iter().map(describe).collect()
    }

    /// 쓸 음성을 고른다. 지정이 없고 문구에 한글이 있으면 한국어 음성을 찾는다.
    fn pick_voice(text: &str, voice_id: Option<&str>) -> Result<Option<VoiceInformation>> {
        let voices = all_voices()?;
        if let Some(id) = voice_id {
            for voice in voices {
                if describe(&voice)?.id == id {
                    return Ok(Some(voice));
                }
            }
            return Err(Error(format!("음성을 찾지 못했습니다: {id}")));
        }
        if contains_hangul(text) {
            for voice in voices {
                if describe(&voice)?.language.to_lowercase().starts_with("ko") {
                    return Ok(Some(voice));
                }
            }
            return Err(Error(
                "한국어 음성이 설치되어 있지 않습니다. Windows 설정 → 시간 및 언어 → 음성에서 한국어 음성을 추가해 주세요."
                    .into(),
            ));
        }
        Ok(None)
    }

    pub fn synthesize(text: &str, voice_id: Option<&str>) -> Result<Vec<u8>> {
        let failed = fail("음성을 만들지 못했습니다");
        let synthesizer = SpeechSynthesizer::new().map_err(&failed)?;
        if let Some(voice) = pick_voice(text, voice_id)? {
            synthesizer.SetVoice(&voice).map_err(&failed)?;
        }

        let stream = synthesizer
            .SynthesizeTextToStreamAsync(&HSTRING::from(text))
            .map_err(&failed)?
            .join()
            .map_err(&failed)?;

        // 합성 결과는 WAV 형식의 스트림이다. 통째로 읽어 온다.
        let size = u32::try_from(stream.Size().map_err(&failed)?)
            .map_err(|_| Error("만들어진 음성이 너무 깁니다.".into()))?;
        let input = stream.GetInputStreamAt(0).map_err(&failed)?;
        let reader = DataReader::CreateDataReader(&input).map_err(&failed)?;
        let load: IAsyncOperation<u32> = reader
            .LoadAsync(size)
            .map_err(&failed)?
            .cast()
            .map_err(&failed)?;
        let loaded = load.join().map_err(&failed)?;
        let mut bytes = vec![0u8; loaded as usize];
        reader.ReadBytes(&mut bytes).map_err(&failed)?;
        Ok(bytes)
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Voice;
    use crate::{Error, Result};

    const UNSUPPORTED: &str = "이 운영체제에서는 음성 합성(TTS)을 아직 지원하지 않습니다.";

    pub fn list_voices() -> Result<Vec<Voice>> {
        Ok(Vec::new())
    }

    pub fn synthesize(_text: &str, _voice_id: Option<&str>) -> Result<Vec<u8>> {
        Err(Error(UNSUPPORTED.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_hangul() {
        assert!(contains_hangul("1.4초, 1.4초 비프"));
        assert!(!contains_hangul("1.4 seconds"));
    }

    #[test]
    fn empty_text_is_rejected() {
        assert!(synthesize("   ", None).is_err());
    }

    /// 실제 OS 음성으로 합성해 본다. 음성이 설치된 Windows 에서만 의미가 있어 기본으로는 건너뛴다.
    ///   cargo test -p beep-native -- --ignored
    #[cfg(windows)]
    #[test]
    #[ignore = "설치된 Windows 음성이 필요합니다"]
    fn synthesizes_wav_with_an_installed_voice() {
        let voices = list_voices().unwrap();
        assert!(!voices.is_empty(), "설치된 음성이 없습니다");
        let wav = synthesize("one point four seconds", Some(&voices[0].id)).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert!(wav.len() > 10_000, "len = {}", wav.len());
    }
}
