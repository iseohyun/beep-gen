//! 코어의 오류 형식.

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// 파라미터가 허용 범위를 벗어났다. `name` 은 CLI 옵션 이름과 같다.
    InvalidParam { name: &'static str, reason: String },
    /// 인코더가 입력을 거부했다.
    Encode(String),
    /// 음악 파일을 읽지 못했다.
    Decode(String),
    /// 음악에서 일정한 박자를 찾지 못했다.
    TempoNotFound,
    /// 템포 변환에 실패했다.
    Stretch(String),
    /// 지원하지 않는 입력이다.
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidParam { name, reason } => write!(f, "잘못된 값 ({name}): {reason}"),
            Error::Encode(msg) => write!(f, "인코딩 실패: {msg}"),
            Error::Decode(msg) => write!(f, "음악 파일을 읽지 못했습니다: {msg}"),
            Error::TempoNotFound => write!(
                f,
                "음악에서 일정한 박자를 찾지 못했습니다. 박자가 뚜렷한 곡으로 시도해 주세요."
            ),
            Error::Stretch(msg) => write!(f, "템포 변환 실패: {msg}"),
            Error::Unsupported(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}
