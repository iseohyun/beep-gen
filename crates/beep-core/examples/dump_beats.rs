//! 진단용: 음악 파일에서 검출한 박을 요약해 보여준다.
//!
//!   cargo run --release -p beep-core --example dump_beats -- <음악 파일> [실제박.txt]
//!
//! 실제 박 시각 파일(한 줄에 초 하나)을 주면, 검출한 박이 실제 박에서 얼마나 벗어나는지도 보여준다.

use timestretch::detect_beat_grid;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("음악 파일 경로가 필요합니다");
    let truth: Option<Vec<f64>> = args.next().map(|p| {
        std::fs::read_to_string(p)
            .expect("실제 박 파일을 읽지 못했습니다")
            .lines()
            .filter_map(|l| l.trim().parse().ok())
            .collect()
    });
    let bytes = std::fs::read(&path).expect("음악 파일을 읽지 못했습니다");
    let audio = beep_core::decode::decode_audio(&bytes).expect("디코딩 실패");
    let rate = f64::from(audio.sample_rate);
    let grid = detect_beat_grid(&audio.to_mono(), audio.sample_rate);

    let beats: Vec<f64> = grid.beats.iter().map(|b| b / rate).collect();
    println!(
        "길이 {:.1}s, bpm {:.3}, 신뢰도 {:.2}, 박 {}개, 구간 {}개, phase_untrusted {}",
        audio.duration_sec(),
        grid.bpm,
        grid.confidence,
        beats.len(),
        grid.segments.len(),
        grid.phase_untrusted
    );
    if beats.len() >= 2 {
        let guess =
            (grid.beats[grid.beats.len() - 1] - grid.beats[0]) / (grid.beats.len() - 1) as f64;
        let mut trace = beep_core::steady::SteadyTrace::default();
        let steady = beep_core::steady::find_steady_grid_traced(
            &audio.to_mono(),
            audio.sample_rate,
            &grid.beats,
            guess,
            beep_core::steady::Meter::Auto,
            Some(&mut trace),
        );
        println!(
            "소리로 찾은 격자: {} (박 간격 {:.3}ms, 또렷한 구간 {}/{}, 또렷함 {:.3}, 구간 사이 최대 이동 {:.1}ms, 전체 이동 폭 {:.1}ms, 킥 겹침 {:.3} → 따라가면 {:.3})",
            match &steady {
                Some(grid) if grid.follows_wavering => "사용(흔들림 따라감)",
                Some(_) => "사용(한 가지 템포)",
                None => "사용 안 함",
            },
            trace.period / rate * 1000.0,
            trace.clear_windows,
            trace.windows,
            trace.clarity,
            trace.max_step / rate * 1000.0,
            trace.drift_range / rate * 1000.0,
            trace.kick_rigid,
            trace.kick_following
        );
    }
    let mut gaps: Vec<f64> = beats.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.is_empty() {
        return;
    }
    let mut sorted = gaps.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    println!(
        "박 간격: 최소 {:.4} 중앙 {:.4} 최대 {:.4} (첫 박 {:.3}s, 끝 박 {:.3}s)",
        sorted[0],
        median,
        sorted[sorted.len() - 1],
        beats[0],
        beats[beats.len() - 1]
    );
    let odd: Vec<String> = gaps
        .iter_mut()
        .enumerate()
        .filter(|(_, g)| (**g / median - 1.0).abs() > 0.2)
        .take(20)
        .map(|(i, g)| format!("#{i}@{:.1}s:{:.3}", beats[i], g))
        .collect();
    println!(
        "간격이 중앙값과 20% 넘게 다른 곳 {}개: {}",
        odd.len(),
        odd.join(" ")
    );

    if let Some(truth) = truth {
        // 구간(30초)마다 검출 박과 가장 가까운 실제 박의 차이(중앙값, ms)
        let mut t0 = 0.0;
        while t0 < audio.duration_sec() {
            let mut errs: Vec<f64> = beats
                .iter()
                .filter(|b| **b >= t0 && **b < t0 + 30.0)
                .map(|b| {
                    truth
                        .iter()
                        .map(|t| b - t)
                        .min_by(|a, c| a.abs().total_cmp(&c.abs()))
                        .unwrap_or(0.0)
                        * 1000.0
                })
                .collect();
            errs.sort_by(f64::total_cmp);
            if !errs.is_empty() {
                println!(
                    "  {:>3.0}~{:>3.0}s: 박 {:>3}개, 오차 중앙 {:+.1}ms (최소 {:+.1}, 최대 {:+.1})",
                    t0,
                    t0 + 30.0,
                    errs.len(),
                    errs[errs.len() / 2],
                    errs[0],
                    errs[errs.len() - 1]
                );
            }
            t0 += 30.0;
        }
    }
}
