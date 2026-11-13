//! 네이티브 실행: native <input.mp3> <output.mp3> [n] [m] [weak]
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: native <input.mp3> <output.mp3> [n=1.4] [m=2] [weak=0.35]");
        std::process::exit(2);
    }
    let n: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.4);
    let m: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(2);
    let weak: f32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(0.35);

    let input = std::fs::read(&args[1]).expect("read input");
    let origin = Instant::now();
    let now = || origin.elapsed().as_secs_f64() * 1000.0;
    match beep_core_spike::process(&input, n, m, weak, &now) {
        Ok((mp3, report)) => {
            std::fs::write(&args[2], mp3).expect("write output");
            println!("{}", report.to_json());
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
