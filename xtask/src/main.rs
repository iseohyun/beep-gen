//! 저장소 빌드 작업.
//!
//!   cargo run -p xtask -- wasm
//!
//! 웹용 연결부(beep-wasm)를 WASM 으로 빌드하고, 브라우저가 불러올 자바스크립트·타입
//! 정의를 `apps/web/src/wasm/` 에 만든다. 바인딩 생성기를 라이브러리로 직접 쓰기 때문에
//! 별도 도구를 설치할 필요가 없고, 버전이 `Cargo.lock` 으로 고정된다.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use wasm_bindgen_cli_support::Bindgen;

const WASM_TARGET: &str = "wasm32-unknown-unknown";
const WASM_CRATE: &str = "beep-wasm";
const WASM_FILE: &str = "beep_wasm.wasm";
const WEB_WASM_DIR: &str = "apps/web/src/wasm";
const WEB_VERSION_FILE: &str = "apps/web/src/version.ts";

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        Some("wasm") => build_wasm(),
        _ => {
            eprintln!("사용법: cargo run -p xtask -- wasm");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("오류: {message}");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask 는 저장소 루트 바로 아래에 있다")
        .to_path_buf()
}

fn build_wasm() -> Result<(), String> {
    let root = repo_root();
    check_web_version(&root)?;

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&root)
        .args([
            "build",
            "--release",
            "-p",
            WASM_CRATE,
            "--target",
            WASM_TARGET,
        ])
        .status()
        .map_err(|e| format!("cargo 를 실행하지 못했습니다: {e}"))?;
    if !status.success() {
        return Err(format!("{WASM_CRATE} 빌드에 실패했습니다"));
    }

    let input = root
        .join("target")
        .join(WASM_TARGET)
        .join("release")
        .join(WASM_FILE);
    let out_dir = root.join(WEB_WASM_DIR);
    Bindgen::new()
        .input_path(&input)
        .web(true)
        .map_err(|e| e.to_string())?
        .typescript(true)
        .generate(&out_dir)
        .map_err(|e| format!("바인딩 생성에 실패했습니다: {e:#}"))?;

    println!("웹용 WASM 생성 완료: {}", out_dir.display());
    Ok(())
}

/// 웹에 표시되는 정식 버전이 Cargo 작업 공간 버전과 같은지 확인한다.
fn check_web_version(root: &Path) -> Result<(), String> {
    let path = root.join(WEB_VERSION_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("{} 을(를) 읽지 못했습니다: {e}", path.display()))?;
    let expected = format!("APP_VERSION = \"{}\"", env!("CARGO_PKG_VERSION"));
    if text.contains(&expected) {
        Ok(())
    } else {
        Err(format!(
            "{WEB_VERSION_FILE} 의 APP_VERSION 이 Cargo 버전({})과 다릅니다",
            env!("CARGO_PKG_VERSION")
        ))
    }
}
