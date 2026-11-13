//! 출력 인코딩: WAV(16비트 PCM)와 MP3.
//!
//! 두 인코더 모두 구간 단위로 샘플을 받아 마지막에 완성된 파일 바이트를 돌려준다.

use crate::error::{Error, Result};
use rusty_mp3::{Mp3Encoder, Mp3EncoderConfig};

/// MP3 고정 비트레이트(kbps).
pub const MP3_BITRATE_KBPS: u32 = 192;

/// 출력 파일 형식.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum OutputFormat {
    #[default]
    Mp3,
    Wav,
}

impl OutputFormat {
    /// 파일 확장자(점 제외).
    pub fn extension(self) -> &'static str {
        match self {
            OutputFormat::Mp3 => "mp3",
            OutputFormat::Wav => "wav",
        }
    }

    /// 확장자로부터 형식을 추정한다(대소문자 무시).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "mp3" => Some(OutputFormat::Mp3),
            "wav" => Some(OutputFormat::Wav),
            _ => None,
        }
    }
}

/// -1.0~1.0 샘플을 16비트 정수로 바꾼다(반올림, 범위 밖은 잘라냄).
fn to_i16(sample: f32) -> i16 {
    let scaled = f64::from(sample) * 32_767.0;
    libm::round(scaled).clamp(-32_768.0, 32_767.0) as i16
}

/// 구간 단위로 샘플을 받는 인코더.
pub enum Encoder {
    Wav(WavEncoder),
    Mp3(Box<Mp3StreamEncoder>),
}

impl Encoder {
    pub fn new(format: OutputFormat, sample_rate: u32, channels: u16) -> Self {
        match format {
            OutputFormat::Wav => Encoder::Wav(WavEncoder::new(sample_rate, channels)),
            OutputFormat::Mp3 => {
                Encoder::Mp3(Box::new(Mp3StreamEncoder::new(sample_rate, channels)))
            }
        }
    }

    /// 채널이 섞여 있는(interleaved) 샘플을 추가한다.
    pub fn push(&mut self, samples: &[f32]) -> Result<()> {
        match self {
            Encoder::Wav(e) => {
                e.push(samples);
                Ok(())
            }
            Encoder::Mp3(e) => e.push(samples),
        }
    }

    /// 인코딩을 끝내고 완성된 파일 바이트를 돌려준다.
    pub fn finish(self) -> Vec<u8> {
        match self {
            Encoder::Wav(e) => e.finish(),
            Encoder::Mp3(e) => e.finish(),
        }
    }
}

/// 16비트 PCM WAV.
pub struct WavEncoder {
    sample_rate: u32,
    channels: u16,
    data: Vec<u8>,
}

const WAV_HEADER_LEN: usize = 44;

impl WavEncoder {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            sample_rate,
            channels,
            data: vec![0; WAV_HEADER_LEN],
        }
    }

    pub fn push(&mut self, samples: &[f32]) {
        self.data.reserve(samples.len() * 2);
        for &s in samples {
            self.data.extend_from_slice(&to_i16(s).to_le_bytes());
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        let data_len = (self.data.len() - WAV_HEADER_LEN) as u32;
        let block_align = self.channels * 2;
        let byte_rate = self.sample_rate * u32::from(block_align);
        let mut header = Vec::with_capacity(WAV_HEADER_LEN);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&(36 + data_len).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&16u32.to_le_bytes()); // fmt 청크 크기
        header.extend_from_slice(&1u16.to_le_bytes()); // PCM
        header.extend_from_slice(&self.channels.to_le_bytes());
        header.extend_from_slice(&self.sample_rate.to_le_bytes());
        header.extend_from_slice(&byte_rate.to_le_bytes());
        header.extend_from_slice(&block_align.to_le_bytes());
        header.extend_from_slice(&16u16.to_le_bytes()); // 비트 수
        header.extend_from_slice(b"data");
        header.extend_from_slice(&data_len.to_le_bytes());
        self.data[..WAV_HEADER_LEN].copy_from_slice(&header);
        self.data
    }
}

/// MP3 (CBR 192kbps).
pub struct Mp3StreamEncoder {
    encoder: Mp3Encoder,
    sample_rate: u32,
    channels: u16,
    out: Vec<u8>,
    /// 첫 프레임(재생 길이 정보 태그)의 길이. 끝난 뒤 완성본으로 덮어쓴다.
    first_packet_len: Option<usize>,
}

impl Mp3StreamEncoder {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            encoder: Mp3Encoder::new(Mp3EncoderConfig {
                bitrate_kbps: MP3_BITRATE_KBPS,
                vbr_quality: None,
            }),
            sample_rate,
            channels,
            out: Vec::new(),
            first_packet_len: None,
        }
    }

    pub fn push(&mut self, samples: &[f32]) -> Result<()> {
        self.encoder
            .push_pcm_f32(samples, self.channels, self.sample_rate)
            .map_err(|e| Error::Encode(format!("{e:?}")))?;
        self.drain();
        Ok(())
    }

    fn drain(&mut self) {
        while let Ok(packet) = self.encoder.next_packet() {
            if self.first_packet_len.is_none() {
                self.first_packet_len = Some(packet.len());
            }
            self.out.extend_from_slice(&packet);
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.encoder.finish();
        self.drain();
        // 첫 프레임은 길이를 모르는 상태로 먼저 나갔으므로, 완성된 태그로 바꿔 넣는다.
        if let (Some(len), Some(tag)) = (self.first_packet_len, self.encoder.lame_tag_frame()) {
            if tag.len() == len && self.out.len() >= len {
                self.out[..len].copy_from_slice(tag);
            }
        }
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_and_samples() {
        let mut enc = WavEncoder::new(44_100, 1);
        enc.push(&[0.0, 1.0, -1.0, 0.5, 2.0]);
        let bytes = enc.finish();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 36 + 10);
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            44_100
        );
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            88_200
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 10);
        let samples: Vec<i16> = bytes[44..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(samples, [0, 32_767, -32_767, 16_384, 32_767]);
    }

    #[test]
    fn mp3_round_trips_to_the_same_length() {
        let rate = 44_100u32;
        let tone: Vec<f32> = (0..rate * 2)
            .map(|i| {
                (libm::sin(std::f64::consts::TAU * 440.0 * f64::from(i) / f64::from(rate)) * 0.5)
                    as f32
            })
            .collect();
        let mut enc = Mp3StreamEncoder::new(rate, 1);
        for chunk in tone.chunks(10_000) {
            enc.push(chunk).unwrap();
        }
        let mp3 = enc.finish();
        // 192kbps × 2초 ≈ 48KB
        assert!((40_000..60_000).contains(&mp3.len()), "len = {}", mp3.len());

        let mut dec = rusty_mp3::Mp3Decoder::new();
        dec.push(&mp3);
        dec.flush();
        let mut decoded = 0usize;
        while let Ok(frame) = dec.next_frame() {
            assert_eq!(frame.sample_rate, rate);
            assert_eq!(frame.channels, 1);
            decoded += frame.samples.len();
        }
        // 디코더는 프레임 단위(1152)로 내보내므로 원본보다 조금 길 수 있다.
        assert!(
            decoded >= tone.len() && decoded < tone.len() + 4 * 1152,
            "decoded = {decoded}"
        );
    }

    #[test]
    fn format_from_extension() {
        assert_eq!(OutputFormat::from_extension("MP3"), Some(OutputFormat::Mp3));
        assert_eq!(OutputFormat::from_extension("wav"), Some(OutputFormat::Wav));
        assert_eq!(OutputFormat::from_extension("ogg"), None);
    }
}
