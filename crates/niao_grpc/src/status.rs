//! gRPC status codes (https://grpc.github.io/grpc/core/md_doc_statuscodes.html).

use crate::error::{GrpcError, GrpcResult};

/// gRPC status code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum StatusCode {
    Ok = 0,
    Cancelled = 1,
    Unknown = 2,
    InvalidArgument = 3,
    DeadlineExceeded = 4,
    NotFound = 5,
    AlreadyExists = 6,
    PermissionDenied = 7,
    ResourceExhausted = 8,
    FailedPrecondition = 9,
    Aborted = 10,
    OutOfRange = 11,
    Unimplemented = 12,
    Internal = 13,
    Unavailable = 14,
    DataLoss = 15,
    Unauthenticated = 16,
}

impl StatusCode {
    pub fn from_i32(v: i32) -> Option<Self> {
        Some(match v {
            0 => Self::Ok,
            1 => Self::Cancelled,
            2 => Self::Unknown,
            3 => Self::InvalidArgument,
            4 => Self::DeadlineExceeded,
            5 => Self::NotFound,
            6 => Self::AlreadyExists,
            7 => Self::PermissionDenied,
            8 => Self::ResourceExhausted,
            9 => Self::FailedPrecondition,
            10 => Self::Aborted,
            11 => Self::OutOfRange,
            12 => Self::Unimplemented,
            13 => Self::Internal,
            14 => Self::Unavailable,
            15 => Self::DataLoss,
            16 => Self::Unauthenticated,
            _ => return None,
        })
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Cancelled => "CANCELLED",
            Self::Unknown => "UNKNOWN",
            Self::InvalidArgument => "INVALID_ARGUMENT",
            Self::DeadlineExceeded => "DEADLINE_EXCEEDED",
            Self::NotFound => "NOT_FOUND",
            Self::AlreadyExists => "ALREADY_EXISTS",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::ResourceExhausted => "RESOURCE_EXHAUSTED",
            Self::FailedPrecondition => "FAILED_PRECONDITION",
            Self::Aborted => "ABORTED",
            Self::OutOfRange => "OUT_OF_RANGE",
            Self::Unimplemented => "UNIMPLEMENTED",
            Self::Internal => "INTERNAL",
            Self::Unavailable => "UNAVAILABLE",
            Self::DataLoss => "DATA_LOSS",
            Self::Unauthenticated => "UNAUTHENTICATED",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_uppercase().as_str() {
            "OK" => Some(Self::Ok),
            "CANCELLED" => Some(Self::Cancelled),
            "UNKNOWN" => Some(Self::Unknown),
            "INVALID_ARGUMENT" => Some(Self::InvalidArgument),
            "DEADLINE_EXCEEDED" => Some(Self::DeadlineExceeded),
            "NOT_FOUND" => Some(Self::NotFound),
            "ALREADY_EXISTS" => Some(Self::AlreadyExists),
            "PERMISSION_DENIED" => Some(Self::PermissionDenied),
            "RESOURCE_EXHAUSTED" => Some(Self::ResourceExhausted),
            "FAILED_PRECONDITION" => Some(Self::FailedPrecondition),
            "ABORTED" => Some(Self::Aborted),
            "OUT_OF_RANGE" => Some(Self::OutOfRange),
            "UNIMPLEMENTED" => Some(Self::Unimplemented),
            "INTERNAL" => Some(Self::Internal),
            "UNAVAILABLE" => Some(Self::Unavailable),
            "DATA_LOSS" => Some(Self::DataLoss),
            "UNAUTHENTICATED" => Some(Self::Unauthenticated),
            _ => None,
        }
    }
}

/// RPC outcome status (code + optional message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub code: StatusCode,
    pub message: String,
}

impl Status {
    pub fn ok() -> Self {
        Self {
            code: StatusCode::Ok,
            message: String::new(),
        }
    }

    pub fn new(code: StatusCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.code == StatusCode::Ok
    }
}

/// Parse `grpc-status` / `grpc-message` from header/trailer maps.
pub fn status_from_headers<'a, I>(headers: I) -> GrpcResult<Status>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    let mut code = None;
    let mut message = String::new();
    for (k, v) in headers {
        let key = k.to_ascii_lowercase();
        if key == "grpc-status" {
            let n: i32 = v
                .trim()
                .parse()
                .map_err(|_| GrpcError::new(format!("invalid grpc-status: {v}")))?;
            code = Some(
                StatusCode::from_i32(n)
                    .ok_or_else(|| GrpcError::new(format!("unknown grpc-status code: {n}")))?,
            );
        } else if key == "grpc-message" {
            message = percent_decode(v);
        }
    }
    Ok(Status {
        code: code.unwrap_or(StatusCode::Ok),
        message,
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Percent-encode a grpc-message trailer value.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b' ' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                const HEX: &[u8] = b"0123456789ABCDEF";
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0xf) as usize] as char);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip_names() {
        for code in 0..=16 {
            let sc = StatusCode::from_i32(code).unwrap();
            assert_eq!(StatusCode::from_name(sc.name()), Some(sc));
        }
    }

    #[test]
    fn parse_trailers() {
        let st = status_from_headers([("grpc-status", "5"), ("grpc-message", "missing")]).unwrap();
        assert_eq!(st.code, StatusCode::NotFound);
        assert_eq!(st.message, "missing");
    }
}
