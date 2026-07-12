#[derive(Debug)]
pub struct Error {
    message: String,
    sqlstate: Option<String>,
}

impl Error {
    pub fn msg(s: impl Into<String>) -> Self {
        Self {
            message: s.into(),
            sqlstate: None,
        }
    }

    pub fn io(e: std::io::Error) -> Self {
        Self::msg(e.to_string())
    }

    pub fn from_error_response(body: &[u8]) -> Self {
        let mut fields = body.split(|&b| b == 0);
        let _typ = fields.next();
        let mut message = String::new();
        let mut sqlstate = None;
        while let Some(chunk) = fields.next() {
            if chunk.is_empty() {
                continue;
            }
            let key = chunk[0];
            let val = std::str::from_utf8(&chunk[1..]).unwrap_or("");
            match key {
                b'M' => message = val.to_string(),
                b'C' => sqlstate = Some(val.to_string()),
                _ => {}
            }
        }
        Self { message, sqlstate }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(code) = &self.sqlstate {
            write!(f, "{} ({code})", self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::io(e)
    }
}
