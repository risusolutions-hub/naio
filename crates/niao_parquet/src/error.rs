use std::fmt;

#[derive(Debug)]
pub enum ParquetError {
    Io(String),
    Arrow(String),
    Parquet(String),
    Schema(String),
    Type(String),
    Shape(String),
}

impl ParquetError {
    pub fn message(&self) -> String {
        match self {
            ParquetError::Io(m) => m.clone(),
            ParquetError::Arrow(m) => m.clone(),
            ParquetError::Parquet(m) => m.clone(),
            ParquetError::Schema(m) => m.clone(),
            ParquetError::Type(m) => m.clone(),
            ParquetError::Shape(m) => m.clone(),
        }
    }
}

impl fmt::Display for ParquetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for ParquetError {}

pub type ParquetResult<T> = Result<T, ParquetError>;

impl From<std::io::Error> for ParquetError {
    fn from(e: std::io::Error) -> Self {
        ParquetError::Io(e.to_string())
    }
}

impl From<arrow_schema::ArrowError> for ParquetError {
    fn from(e: arrow_schema::ArrowError) -> Self {
        ParquetError::Arrow(e.to_string())
    }
}

impl From<parquet::errors::ParquetError> for ParquetError {
    fn from(e: parquet::errors::ParquetError) -> Self {
        ParquetError::Parquet(e.to_string())
    }
}
