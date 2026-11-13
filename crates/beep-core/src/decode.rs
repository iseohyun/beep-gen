//! 음악 파일 디코딩.
//!
//! 파일 바이트를 받아 -1.0~1.0 범위의 샘플로 푼다. 형식은 내용으로 판별하므로
//! 파일 이름이나 확장자는 필요 없다. 지원: MP3, WAV, FLAC, OGG(Vorbis), M4A(AAC).

use std::io::Cursor;

use symphonia::core::audio::sample::Sample;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTag};

use crate::error::{Error, Result};
use crate::naming::SongInfo;

/// 디코딩된 오디오. 채널은 1(모노) 또는 2(스테레오)이며, 샘플은 채널이 번갈아 나온다.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub sample_rate: u32,
    pub channels: usize,
    pub samples: Vec<f32>,
}

impl Audio {
    /// 길이(샘플 프레임 수). 스테레오는 좌우 한 쌍이 한 프레임이다.
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }

    pub fn duration_sec(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    /// 채널을 평균해 모노로 만든다(박자 분석용).
    pub fn to_mono(&self) -> Vec<f32> {
        if self.channels == 1 {
            return self.samples.clone();
        }
        self.samples
            .chunks_exact(self.channels)
            .map(|frame| frame.iter().sum::<f32>() / self.channels as f32)
            .collect()
    }

    /// 가장 큰 진폭(절댓값).
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }
}

/// 파일 내용을 열어 형식을 판별한다.
fn open(bytes: &[u8]) -> Result<Box<dyn FormatReader>> {
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    symphonia::default::get_probe()
        .probe(
            &Hint::new(),
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| Error::Decode(format!("지원하지 않는 파일 형식입니다 ({e})")))
}

/// 음악 파일에 적힌 가수와 곡 제목(ID3 등의 태그)을 읽는다.
///
/// 소리는 풀지 않으므로 빠르다. 태그가 없거나 읽을 수 없는 파일이면 빈 값을 돌려준다.
pub fn read_song_tags(bytes: &[u8]) -> SongInfo {
    let Ok(mut format) = open(bytes) else {
        return SongInfo::default();
    };
    let mut song = SongInfo::default();
    let mut metadata = format.metadata();
    // 태그는 여러 묶음(파일 앞·뒤, 형식별)으로 올 수 있다. 먼저 나온 값을 쓴다.
    loop {
        if let Some(revision) = metadata.current() {
            let containers = std::iter::once(&revision.media)
                .chain(revision.per_track.iter().map(|track| &track.metadata));
            for tag in containers.flat_map(|container| &container.tags) {
                match &tag.std {
                    Some(StandardTag::Artist(value)) if song.artist.is_none() => {
                        song.artist = Some(value.to_string());
                    }
                    Some(StandardTag::TrackTitle(value)) if song.title.is_none() => {
                        song.title = Some(value.to_string());
                    }
                    _ => {}
                }
            }
        }
        if metadata.pop().is_none() {
            break;
        }
    }
    song.cleaned()
}

/// 음악 파일 바이트를 디코딩한다.
///
/// 3채널 이상은 앞의 두 채널(보통 좌·우)만 쓴다.
pub fn decode_audio(bytes: &[u8]) -> Result<Audio> {
    let mut format = open(bytes)?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| Error::Decode("파일에 오디오가 없습니다".into()))?;
    let track_id = track.id;
    // 전체 길이를 알면 버퍼를 한 번에 잡는다. 조금씩 늘리면 늘릴 때마다 통째로 옮겨야 해서
    // 순간적으로 두 배 가까운 메모리를 쓴다(웹에서는 한번 늘어난 메모리가 줄지 않는다).
    let expected_frames = track.num_frames;
    let codec_params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| Error::Decode("오디오 정보를 읽지 못했습니다".into()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
        .map_err(|e| Error::Decode(format!("지원하지 않는 코덱입니다 ({e})")))?;

    let mut sample_rate = 0u32;
    let mut source_channels = 0usize;
    let mut samples: Vec<f32> = Vec::new();
    let mut packet_samples: Vec<f32> = Vec::new();

    // 파일 끝이나 손상된 꼬리는 오류로 돌아오므로, 그때까지 푼 것으로 마무리한다.
    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }
        let buffer = match decoder.decode(&packet) {
            Ok(buffer) => buffer,
            // 깨진 구간 하나는 건너뛴다.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(_) => break,
        };
        let channels = buffer.spec().channels().count();
        if channels == 0 {
            continue;
        }
        if sample_rate == 0 {
            sample_rate = buffer.spec().rate();
            source_channels = channels;
            if let Some(frames) = expected_frames {
                samples.reserve_exact(frames as usize * channels.min(2));
            }
        } else if channels != source_channels {
            return Err(Error::Decode(
                "중간에 채널 수가 바뀌는 파일은 지원하지 않습니다".into(),
            ));
        }

        packet_samples.resize(buffer.samples_interleaved(), f32::MID);
        buffer.copy_to_slice_interleaved(&mut packet_samples);
        if channels <= 2 {
            samples.extend_from_slice(&packet_samples);
        } else {
            for frame in packet_samples.chunks_exact(channels) {
                samples.extend_from_slice(&frame[..2]);
            }
        }
    }

    if samples.is_empty() || sample_rate == 0 {
        return Err(Error::Decode("오디오를 읽지 못했습니다".into()));
    }
    // 손상된 데이터에서 나올 수 있는 비정상 값을 정리한다.
    for sample in &mut samples {
        *sample = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
    }
    Ok(Audio {
        sample_rate,
        channels: source_channels.min(2),
        samples,
    })
}
