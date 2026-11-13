//! 코어의 오류 형식.

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// 파라미터가 허용 범위를 벗어났다. `name` 은 CLI 옵션 이름과 같다.
    InvalidParam { name: &'static str, reason: String },
    /// 인코더가 입력을 거부했다.
    Encode(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidParam { name, reason } => write!(f, "잘못된 값 ({name}): {reason}"),
            Error::Encode(msg) => write!(f, "인코딩 실패: {msg}"),
        }
    }
}

impl std::error::Error for Error {}
