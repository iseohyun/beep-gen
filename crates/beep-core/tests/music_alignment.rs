//! 배경 음악의 박이 비프와 맞는지 끝에서 끝까지 확인한다.
//!
//! 100 BPM 의 단순한 곡을 합성해 1.4초·2분할 비프(간격 0.7초)에 맞추게 한 뒤,
//! "음악만" 과 "비프만" 두 결과에서 박의 시작 시각을 찾아 비교한다.

use beep_core::encode::WavEncoder;
use beep_core::{Excerpt, OutputFormat, TrackInputs, TrackOutput, TrackParams, build_track};

const RATE: u32 = 44_100;
const SOURCE_BPM: f64 = 100.0;
const SOURCE_SECONDS: usize = 60;

/// 템포가 일정한 곡.
fn synth_music() -> Vec<u8> {
    synth_music_with(SOURCE_SECONDS, |_| SOURCE_BPM)
}

/// 박마다 킥(낮은 음의 짧은 울림), 박 사이마다 약한 잡음이 있는 스테레오 곡을 WAV 로 만든다.
/// `bpm_at(초)` 로 그 시점의 템포를 정한다.
fn synth_music_with(seconds: usize, bpm_at: impl Fn(f64) -> f64) -> Vec<u8> {
    let frames = RATE as usize * seconds;
    let mut mono = vec![0.0f32; frames];

    let kick_len = (0.15 * f64::from(RATE)) as usize;
    let hat_len = (0.03 * f64::from(RATE)) as usize;
    let mut seed = 12_345u32;
    let mut noise = || {
        // 실행할 때마다 같은 값이 나오는 간단한 난수.
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (f64::from(seed >> 8) / f64::from(1u32 << 24)) * 2.0 - 1.0
    };

    let mut time = 0.0f64;
    loop {
        let start = (time * f64::from(RATE)).round() as usize;
        if start >= frames {
            break;
        }
        let beat = 60.0 / bpm_at(time) * f64::from(RATE);
        for i in 0..kick_len.min(frames - start) {
            let t = i as f64 / f64::from(RATE);
            mono[start + i] +=
                ((std::f64::consts::TAU * 60.0 * t).sin() * (-t * 30.0).exp() * 0.8) as f32;
        }
        let half = start + (beat / 2.0) as usize;
        if half < frames {
            for i in 0..hat_len.min(frames - half) {
                let t = i as f64 / f64::from(RATE);
                mono[half + i] += (noise() * (-t * 150.0).exp() * 0.1) as f32;
            }
        }
        time += 60.0 / bpm_at(time);
    }

    let stereo: Vec<f32> = mono.iter().flat_map(|s| [*s, *s]).collect();
    let mut encoder = WavEncoder::new(RATE, 2);
    encoder.push(&stereo);
    encoder.finish()
}

/// 16비트 스테레오 WAV 에서 왼쪽 채널을 -1~1 로 읽는다.
fn left_channel(wav: &[u8]) -> Vec<f32> {
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(
        u16::from_le_bytes([wav[22], wav[23]]),
        2,
        "스테레오여야 합니다"
    );
    wav[44..]
        .chunks_exact(4)
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
        .collect()
}

/// 조용한 구간 뒤에 `threshold` 를 넘는 지점들(샘플 위치).
fn onsets(samples: &[f32], threshold: f32, min_gap: usize) -> Vec<usize> {
    let mut found = Vec::new();
    let mut last_loud: Option<usize> = None;
    for (i, sample) in samples.iter().enumerate() {
        if sample.abs() >= threshold {
            if last_loud.is_none_or(|last| i - last >= min_gap) {
                found.push(i);
            }
            last_loud = Some(i);
        }
    }
    found
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

/// 같은 설정으로 "음악만" 과 "비프만" 두 음원을 만든다(1.4초·2분할 = 비프 간격 0.7초).
fn render_pair(music: &[u8], excerpt: Option<Excerpt>) -> (TrackOutput, TrackOutput) {
    let base = TrackParams {
        interval_sec: 1.4,
        subdivision: 2,
        ..TrackParams::default()
    };
    let render = |params: TrackParams| {
        let inputs = TrackInputs {
            music: Some(music),
            excerpt,
            ..TrackInputs::default()
        };
        build_track(&params, OutputFormat::Wav, inputs, |_| {}).unwrap()
    };
    let music_only = render(TrackParams {
        beep_volume_pct: 0.0,
        music_volume_pct: 100.0,
        ..base.clone()
    });
    let beeps_only = render(TrackParams {
        beep_volume_pct: 100.0,
        music_volume_pct: 0.0,
        ..base
    });
    (music_only, beeps_only)
}

/// 킥마다 가장 가까운 비프와의 시간 차(ms). 양수면 음악이 늦다.
fn kick_offsets(music_audio: &[f32], beep_audio: &[f32]) -> Vec<f64> {
    let gap = RATE as usize / 5;
    let beeps = onsets(beep_audio, 0.01, gap);
    let peak = music_audio.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    // 박 사이의 약한 소리(킥의 1/8 크기)가 킥으로 세어지지 않게 문턱을 넉넉히 둔다.
    let kicks = onsets(music_audio, peak * 0.5, gap);
    kicks
        .iter()
        .map(|kick| {
            let nearest = beeps
                .iter()
                .min_by_key(|beep| beep.abs_diff(*kick))
                .unwrap();
            (*kick as f64 - *nearest as f64) / f64::from(RATE) * 1000.0
        })
        .collect()
}

/// 템포가 조금씩 변하는 곡(사람이 연주한 곡)도 끝까지 비프와 맞아야 한다.
/// 곡 전체를 한 가지 템포로 보면 이 곡은 뒤로 갈수록 반 박 가까이 어긋난다.
#[test]
fn drifting_tempo_is_followed_to_the_end() {
    let seconds = 150;
    let music = synth_music_with(seconds, |t| 100.0 + 3.0 * t / seconds as f64);
    let (music_only, beeps_only) = render_pair(&music, None);
    let report = music_only.music.as_ref().unwrap();
    assert!(report.tempo_drift_pct > 1.0, "{report:?}");

    let offsets = kick_offsets(
        &left_channel(&music_only.bytes),
        &left_channel(&beeps_only.bytes),
    );
    assert!(offsets.len() > 200, "킥 {}개", offsets.len());
    // 곡의 맨 앞·맨 끝 몇 박은 양옆에 볼 박이 적어 덜 정확하므로 뺀다.
    let body = &offsets[8..offsets.len() - 8];
    let worst = body.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(worst < 30.0, "가장 크게 어긋난 박: {worst:.1}ms");
    let quarter = body.len() / 4;
    let (first, last) = (
        median(&body[..quarter]),
        median(&body[body.len() - quarter..]),
    );
    assert!(
        (last - first).abs() < 5.0,
        "뒤로 갈수록 어긋납니다: 앞 {first:.1}ms → 뒤 {last:.1}ms"
    );
}

/// 미리듣기: 곡의 중간 10초만 만들어도 박이 맞고, 강한 비프에서 시작한다.
#[test]
fn excerpt_is_aligned_and_starts_on_a_strong_beep() {
    let music = synth_music();
    let excerpt = Excerpt {
        seconds: 10.0,
        from_percent: 50.0,
    };
    let (music_only, beeps_only) = render_pair(&music, Some(excerpt));
    let music_audio = left_channel(&music_only.bytes);
    let beep_audio = left_channel(&beeps_only.bytes);
    assert_eq!(music_audio.len(), 10 * RATE as usize);
    assert_eq!(beep_audio.len(), music_audio.len());
    assert!((music_only.music.as_ref().unwrap().output_seconds - 10.0).abs() < 1e-9);

    // 첫 비프는 맨 앞에서 시작하고, 강한 비프(880Hz)다: 약한 비프보다 크다.
    let beeps = onsets(&beep_audio, 0.01, RATE as usize / 5);
    assert!(beeps[0] < 50, "첫 비프 위치 {}", beeps[0]);
    let loudness = |start: usize| {
        beep_audio[start..start + 2000]
            .iter()
            .fold(0.0f32, |m, v| m.max(v.abs()))
    };
    assert!(loudness(beeps[0]) > loudness(beeps[1]) * 1.5);

    let offsets = kick_offsets(&music_audio, &beep_audio);
    assert!(offsets.len() >= 13, "킥 {}개", offsets.len());
    let worst = offsets.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(worst < 15.0, "가장 크게 어긋난 박: {worst:.1}ms");

    // 곡보다 긴 미리듣기를 청해도 곡 길이를 넘지 않는다.
    let long = Excerpt {
        seconds: 120.0,
        from_percent: 90.0,
    };
    let (music_only, _) = render_pair(&music, Some(long));
    let full_seconds = SOURCE_SECONDS as f64 * 0.7 / 0.6;
    let got = left_channel(&music_only.bytes).len() as f64 / f64::from(RATE);
    assert!((got - full_seconds).abs() < 0.1, "{got}초");
}

#[test]
fn music_beats_stay_on_the_beep_grid() {
    let music = synth_music();
    let (music_only, beeps_only) = render_pair(&music, None);

    // 보고 내용: 100 BPM(박 0.6초)을 0.7초에 맞추므로 약 14% 느려진다.
    let report = music_only
        .music
        .as_ref()
        .expect("음악 보고가 있어야 합니다");
    assert!((report.detected_bpm - SOURCE_BPM).abs() < 0.1, "{report:?}");
    assert!((report.target_beat_sec - 0.7).abs() < 1e-9, "{report:?}");
    assert!((report.tempo_rate - 0.6 / 0.7).abs() < 1e-3, "{report:?}");
    let expected_seconds = SOURCE_SECONDS as f64 * 0.7 / 0.6;
    assert!(
        (report.output_seconds - expected_seconds).abs() < 0.1,
        "{report:?}"
    );
    assert_eq!(
        beeps_only.music.as_ref(),
        Some(report),
        "같은 음악이면 보고도 같아야 합니다"
    );

    let music_audio = left_channel(&music_only.bytes);
    let beep_audio = left_channel(&beeps_only.bytes);
    let actual_seconds = music_audio.len() as f64 / f64::from(RATE);
    assert!(
        (actual_seconds - expected_seconds).abs() < 0.1,
        "길이 {actual_seconds}초"
    );
    assert_eq!(music_audio.len(), beep_audio.len());

    // 비프는 0.7초마다, 킥도 (맞춰졌다면) 0.7초마다 시작한다.
    let offsets = kick_offsets(&music_audio, &beep_audio);
    assert!(offsets.len() >= 95, "킥 {}개", offsets.len());
    let quarter = offsets.len() / 4;
    let first = median(&offsets[..quarter]);
    let last = median(&offsets[offsets.len() - quarter..]);

    assert!(
        first.abs() < 15.0 && last.abs() < 15.0,
        "음악과 비프가 어긋났습니다: 앞 {first:.1}ms, 뒤 {last:.1}ms"
    );
    assert!(
        (last - first).abs() < 3.0,
        "뒤로 갈수록 어긋납니다: 앞 {first:.1}ms → 뒤 {last:.1}ms"
    );
}

#[test]
fn music_without_a_beat_is_rejected() {
    // 5초짜리 순수한 사인파: 박이 없다.
    let frames = RATE as usize * 5;
    let tone: Vec<f32> = (0..frames)
        .map(|i| ((std::f64::consts::TAU * 440.0 * i as f64 / f64::from(RATE)).sin() * 0.5) as f32)
        .collect();
    let mut encoder = WavEncoder::new(RATE, 1);
    encoder.push(&tone);
    let wav = encoder.finish();

    let result = build_track(
        &TrackParams::default(),
        OutputFormat::Wav,
        TrackInputs {
            music: Some(&wav),
            ..TrackInputs::default()
        },
        |_| {},
    );
    assert_eq!(result.unwrap_err(), beep_core::Error::TempoNotFound);
}
