//! 로컬 환경 어댑터.
//!
//! 코어가 직접 할 수 없는 일(OS 호출, 네트워크)을 맡는다. CLI·데스크톱 앱·MCP 가 함께 쓴다.
//! 여기에는 소리를 만드는 계산을 두지 않는다. 가져오기와 OS 호출만 한다.
//! (규칙: `.agents/rules/project_architecture.md` 2.2절)

pub mod fetch;
pub mod job;
pub mod tts;

use std::fmt;

/// 어댑터의 오류. 사용자에게 그대로 보여줄 수 있는 문장을 담는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
