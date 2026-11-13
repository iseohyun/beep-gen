//! 검증용 최소 코어: MP3 디코딩 → 박자 분석 → 템포 변환 → 비프 겹치기 → MP3 인코딩.
//!
//! 이 파일은 파일·네트워크·시계 같은 플랫폼 기능을 직접 쓰지 않는다.
//! 시간 측정용 시계만 호출자가 `now` 로 넘겨준다(네이티브: Instant, 웹: performance.now).

use rusty_mp3::{Error as Mp3Error, Mp3Decoder, Mp3Encoder, Mp3EncoderConfig};
use timestretch::{Backend, Parameters, SessionConfig, TempoRate, detect_beat_grid, render};

/// 단계별 소요 시간(ms)과 결과 요약.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub decode_ms: f64,
    pub analyze_ms: f64,
    pub stretch_ms: f64,
    pub mix_ms: f64,
    pub encode_ms: f64,
    pub sample_rate: u32,
    pub channels: usize,
    pub source_seconds: f64,
    pub output_seconds: f64,
    pub detected_bpm: f64,
    pub beat_confidence: f32,
    /// 음악의 한 박이 맞춰진 목표 간격(초).
    pub target_beat_sec: f64,
    /// 적용된 속도 배율(1보다 크면 빨라짐).
    pub tempo_rate: f64,
    /// 박 위치 전체를 직선으로 맞춰 구한 원곡의 박 간격(샘플).
    pub source_beat_samples: f64,
    /// 라이브러리가 실제로 적용한 길이 배율.
    pub achieved_stretch: f64,
    /// 남은 오차를 없애려고 추가로 적용한 미세 길이 배율(1이면 보정 없음).
    pub correction: f64,
    /// 검출된 템포 구간 수(1이면 일정한 템포).
    pub tempo_segments: usize,
    pub resample_ms: f64,
    pub mp3_bytes: usize,
    /// 변환된 PCM 의 FNV-1a 해시(웹·네이티브 결과 비교용).
    pub pcm_hash: u64,
    /// 출력 MP3 의 FNV-1a 해시.
    pub mp3_hash: u64,
}

impl Report {
    pub fn total_ms(&self) -> f64 {
        self.decode_ms
            + self.analyze_ms
            + self.stretch_ms
            + self.resample_ms
            + self.mix_ms
            + self.encode_ms
    }

    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\"decode_ms\":{:.1},\"analyze_ms\":{:.1},\"stretch_ms\":{:.1},",
                "\"mix_ms\":{:.1},\"encode_ms\":{:.1},\"total_ms\":{:.1},",
                "\"sample_rate\":{},\"channels\":{},\"source_seconds\":{:.2},",
                "\"output_seconds\":{:.2},\"detected_bpm\":{:.3},\"beat_confidence\":{:.3},",
                "\"target_beat_sec\":{:.4},\"tempo_rate\":{:.5},\"mp3_bytes\":{},",
                "\"source_beat_samples\":{:.4},\"achieved_stretch\":{:.8},",
                "\"correction\":{:.8},\"tempo_segments\":{},\"resample_ms\":{:.1},",
                "\"pcm_hash\":\"{:016x}\",\"mp3_hash\":\"{:016x}\"}}"
            ),
            self.decode_ms,
            self.analyze_ms,
            self.stretch_ms,
            self.mix_ms,
            self.encode_ms,
            self.total_ms(),
            self.sample_rate,
            self.channels,
            self.source_seconds,
            self.output_seconds,
            self.detected_bpm,
            self.beat_confidence,
            self.target_beat_sec,
            self.tempo_rate,
            self.mp3_bytes,
            self.source_beat_samples,
            self.achieved_stretch,
            self.correction,
            self.tempo_segments,
            self.resample_ms,
            self.pcm_hash,
            self.mp3_hash,
        )
    }
}

fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// 비프 주기 `n`초, 분할 `m`개에 음악을 맞춘다.
///
/// 음악의 한 박을 분할 간격(n/m)의 0.5·1·2배 중 변환 폭이 가장 작은 쪽에 맞춘다.
pub fn process(
    mp3_in: &[u8],
    n: f64,
    m: u32,
    weak_volume: f32,
    now: &dyn Fn() -> f64,
) -> Result<(Vec<u8>, Report), String> {
    let mut report = Report::default();

    // 1. 디코딩
    let t = now();
    let mut decoder = Mp3Decoder::new();
    decoder.push(mp3_in);
    decoder.flush();
    let mut pcm: Vec<f32> = Vec::new();
    let mut sample_rate = 0u32;
    let mut channels = 0usize;
    loop {
        match decoder.next_frame() {
            Ok(frame) => {
                sample_rate = frame.sample_rate;
                channels = frame.channels as usize;
                pcm.extend_from_slice(&frame.samples);
            }
            Err(Mp3Error::Again | Mp3Error::Eof) => break,
            Err(e) => return Err(format!("decode: {e:?}")),
        }
    }
    if pcm.is_empty() || channels == 0 {
        return Err("decode: no audio".into());
    }
    report.decode_ms = now() - t;
    report.sample_rate = sample_rate;
    report.channels = channels;
    let source_frames = pcm.len() / channels;
    report.source_seconds = source_frames as f64 / sample_rate as f64;

    // 2. 박자 분석 (모노로 합쳐서)
    let t = now();
    let mono: Vec<f32> = pcm
        .chunks_exact(channels)
        .map(|f| f.iter().sum::<f32>() / channels as f32)
        .collect();
    let grid = detect_beat_grid(&mono, sample_rate);
    drop(mono);
    report.analyze_ms = now() - t;
    report.detected_bpm = grid.bpm;
    report.beat_confidence = grid.confidence;
    if grid.bpm <= 0.0 || grid.beats.is_empty() {
        return Err("analyze: tempo not detected".into());
    }

    // 3. 목표 박 간격 결정 → 템포 변환
    // 대표 BPM(중앙값) 대신 모든 박 위치에 직선을 맞춰 간격과 첫 박 위치를 구한다.
    // 박 하나의 오차는 작아도 수백 박이 쌓이면 곡 끝에서 크게 어긋나기 때문이다.
    let (beat_samples, first_beat) = fit_beat_line(&grid.beats)
        .unwrap_or((60.0 * f64::from(sample_rate) / grid.bpm, grid.beats[0]));
    report.source_beat_samples = beat_samples;
    report.tempo_segments = grid.segments.len();
    let source_beat_sec = beat_samples / f64::from(sample_rate);
    let step = n / f64::from(m.max(1));
    let (target_beat_sec, tempo_rate) = [0.5, 1.0, 2.0]
        .iter()
        .map(|k| {
            let target = step * k;
            (target, source_beat_sec / target)
        })
        .min_by(|a, b| a.1.ln().abs().total_cmp(&b.1.ln().abs()))
        .unwrap();
    report.target_beat_sec = target_beat_sec;
    report.tempo_rate = tempo_rate;

    let t = now();
    let rate = TempoRate::new(tempo_rate).map_err(|e| format!("tempo: {e:?}"))?;
    let rendered = render(
        &pcm,
        SessionConfig {
            source_sample_rate: sample_rate,
            output_sample_rate: sample_rate,
            channels,
            backend: Backend::Portable,
            parameters: Parameters::keylock(rate),
            ..SessionConfig::default()
        },
    )
    .map_err(|e| format!("stretch: {e:?}"))?;
    drop(pcm);
    let achieved = rendered.report.parameters.achieved_stretch;
    report.achieved_stretch = achieved;
    let mut out = rendered.samples;
    report.stretch_ms = now() - t;

    // 3-1. 미세 보정: 라이브러리는 배율을 내부 단위로 반올림하므로 요청한 값과 조금 다르다.
    // 실제 박 간격과 목표의 비율만큼 아주 약간 다시 샘플링해 정확히 맞춘다.
    // (측정된 보정 폭은 0.06~0.16%, 음정으로 약 1~3센트다.)
    let t = now();
    let actual_beat = beat_samples * achieved;
    let wanted_beat = target_beat_sec * f64::from(sample_rate);
    let correction = wanted_beat / actual_beat;
    report.correction = correction;
    if (correction - 1.0).abs() > 1e-7 {
        out = resample(&out, channels, correction);
    }
    report.resample_ms = now() - t;

    // 4. 첫 박이 0초에 오도록 앞을 자르고, 비프를 겹친다
    let t = now();
    let first_beat_out = (first_beat * achieved * correction).round() as usize;
    let cut = (first_beat_out * channels).min(out.len());
    out.drain(..cut);
    overlay_beeps(&mut out, sample_rate, channels, n, m, weak_volume);
    report.mix_ms = now() - t;
    report.output_seconds = (out.len() / channels) as f64 / sample_rate as f64;

    let mut h = FNV_OFFSET;
    for s in &out {
        fnv1a(&mut h, &s.to_bits().to_le_bytes());
    }
    report.pcm_hash = h;

    // 5. MP3 인코딩
    let t = now();
    let mut encoder = Mp3Encoder::new(Mp3EncoderConfig {
        bitrate_kbps: 192,
        vbr_quality: None,
    });
    encoder
        .push_pcm_f32(&out, channels as u16, sample_rate)
        .map_err(|e| format!("encode: {e:?}"))?;
    encoder.finish();
    let mut mp3 = Vec::new();
    while let Ok(packet) = encoder.next_packet() {
        mp3.extend_from_slice(&packet);
    }
    report.encode_ms = now() - t;
    report.mp3_bytes = mp3.len();
    let mut h = FNV_OFFSET;
    fnv1a(&mut h, &mp3);
    report.mp3_hash = h;

    Ok((mp3, report))
}

/// 박 번호 i 와 위치 beats[i] 에 최소제곱 직선을 맞춘다.
/// 반환: (박 간격, 첫 박 위치) — 둘 다 샘플 단위. 첫 박 위치는 0 이상이 되도록 한 박씩 민다.
fn fit_beat_line(beats: &[f64]) -> Option<(f64, f64)> {
    let count = beats.len();
    if count < 8 {
        return None;
    }
    let nf = count as f64;
    let mean_i = (nf - 1.0) / 2.0;
    let mean_p = beats.iter().sum::<f64>() / nf;
    let mut num = 0.0;
    let mut den = 0.0;
    for (i, p) in beats.iter().enumerate() {
        let di = i as f64 - mean_i;
        num += di * (p - mean_p);
        den += di * di;
    }
    let slope = num / den;
    if !(slope.is_finite() && slope > 0.0) {
        return None;
    }
    let mut first = mean_p - slope * mean_i;
    while first < 0.0 {
        first += slope;
    }
    Some((slope, first))
}

/// 길이를 `ratio` 배로 바꾸는 재샘플링(창 함수 sinc 보간, 32탭).
/// 배율이 1에 아주 가까운 경우만 쓰므로 별도의 저역 필터는 두지 않는다.
fn resample(input: &[f32], channels: usize, ratio: f64) -> Vec<f32> {
    const TAPS: usize = 32;
    const PHASES: usize = 1024;
    let half = (TAPS / 2) as isize;
    // 위상별 계수표: table[p][k] = 위치 (k - half + 1 - p/PHASES) 에서의 창 함수 sinc
    let mut table = vec![0.0f32; (PHASES + 1) * TAPS];
    for p in 0..=PHASES {
        let frac = p as f64 / PHASES as f64;
        let mut sum = 0.0;
        for k in 0..TAPS {
            let x = (k as isize - half + 1) as f64 - frac;
            let sinc = if x.abs() < 1e-12 {
                1.0
            } else {
                let px = std::f64::consts::PI * x;
                libm_sin(px) / px
            };
            // 블랙먼 창
            let w = x / half as f64;
            let window = if w.abs() >= 1.0 {
                0.0
            } else {
                let c = std::f64::consts::PI * w;
                0.42 + 0.5 * libm_cos(c) + 0.08 * libm_cos(2.0 * c)
            };
            let v = sinc * window;
            table[p * TAPS + k] = v as f32;
            sum += v;
        }
        for k in 0..TAPS {
            table[p * TAPS + k] = (f64::from(table[p * TAPS + k]) / sum) as f32;
        }
    }

    let in_frames = (input.len() / channels) as isize;
    let out_frames = (in_frames as f64 * ratio).round() as usize;
    let step = 1.0 / ratio;
    let mut out = vec![0.0f32; out_frames * channels];
    for (o, frame) in out.chunks_exact_mut(channels).enumerate() {
        let pos = o as f64 * step;
        let base = pos.floor();
        let phase = ((pos - base) * PHASES as f64).round() as usize;
        let coeffs = &table[phase * TAPS..(phase + 1) * TAPS];
        let start = base as isize - half + 1;
        for (k, &c) in coeffs.iter().enumerate() {
            let idx = start + k as isize;
            if idx < 0 || idx >= in_frames {
                continue;
            }
            let src = &input[idx as usize * channels..idx as usize * channels + channels];
            for ch in 0..channels {
                frame[ch] += src[ch] * c;
            }
        }
    }
    out
}

fn libm_cos(x: f64) -> f64 {
    libm_sin(x + std::f64::consts::FRAC_PI_2)
}

/// 기존 generator.py 와 같은 규칙의 비프(강박 880Hz, 약박 587.33Hz, 80ms, 5ms 페이드)를 더한다.
fn overlay_beeps(out: &mut [f32], sample_rate: u32, channels: usize, n: f64, m: u32, weak: f32) {
    let sr = f64::from(sample_rate);
    let m = m.max(1);
    let step = n / f64::from(m);
    let dur = 0.08f64.min(step * 0.6);
    let len = (dur * sr) as usize;
    let fade = ((0.005 * sr) as usize).min(len / 2).max(1);
    let frames = out.len() / channels;
    let mut beat = 0u64;
    loop {
        let start = (beat as f64 * step * sr).round() as usize;
        if start >= frames {
            break;
        }
        let primary = beat % u64::from(m) == 0;
        let (freq, vol) = if primary { (880.0, 0.6) } else { (587.33, 0.6 * f64::from(weak)) };
        for i in 0..len.min(frames - start) {
            let env = if i < fade {
                i as f64 / fade as f64
            } else if i >= len - fade {
                (len - 1 - i) as f64 / fade as f64
            } else {
                1.0
            };
            let v = (libm_sin(std::f64::consts::TAU * freq * i as f64 / sr) * env * vol) as f32;
            for c in 0..channels {
                let s = &mut out[(start + i) * channels + c];
                *s = (*s + v).clamp(-1.0, 1.0);
            }
        }
        beat += 1;
    }
}

/// 표준 `sin` 은 환경마다 구현이 달라 결과가 미세하게 달라질 수 있어,
/// 양쪽에서 같은 값이 나오도록 직접 계산한다(테일러 급수, 구간 축소).
fn libm_sin(x: f64) -> f64 {
    use std::f64::consts::{PI, TAU};
    let mut r = x % TAU;
    if r > PI {
        r -= TAU;
    } else if r < -PI {
        r += TAU;
    }
    let r2 = r * r;
    let mut term = r;
    let mut sum = r;
    let mut k = 1.0;
    for _ in 0..12 {
        term *= -r2 / ((k + 1.0) * (k + 2.0));
        sum += term;
        k += 2.0;
    }
    sum
}

// ───────────────────────── 웹(WASM) 연결부 ─────────────────────────
#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::process;
    use std::cell::RefCell;

    #[link(wasm_import_module = "env")]
    unsafe extern "C" {
        fn js_now() -> f64;
    }

    thread_local! {
        static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        static REPORT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn alloc(len: usize) -> *mut u8 {
        let mut buf = Vec::<u8>::with_capacity(len);
        let ptr = buf.as_mut_ptr();
        std::mem::forget(buf);
        ptr
    }

    /// 성공 1, 실패 0. 실패 시 REPORT 에 오류 문자열이 들어간다.
    #[unsafe(no_mangle)]
    pub extern "C" fn run(ptr: *mut u8, len: usize, n: f64, m: u32, weak: f32) -> u32 {
        let input = unsafe { Vec::from_raw_parts(ptr, len, len) };
        let now = || unsafe { js_now() };
        match process(&input, n, m, weak, &now) {
            Ok((mp3, report)) => {
                OUTPUT.with(|o| *o.borrow_mut() = mp3);
                REPORT.with(|r| *r.borrow_mut() = report.to_json().into_bytes());
                1
            }
            Err(e) => {
                REPORT.with(|r| *r.borrow_mut() = e.into_bytes());
                0
            }
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn output_ptr() -> *const u8 {
        OUTPUT.with(|o| o.borrow().as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn output_len() -> usize {
        OUTPUT.with(|o| o.borrow().len())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn report_ptr() -> *const u8 {
        REPORT.with(|r| r.borrow().as_ptr())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn report_len() -> usize {
        REPORT.with(|r| r.borrow().len())
    }
}
