use super::error::Error;
use super::types::FromSql;

#[derive(Debug, Clone)]
pub struct Row {
    columns: Vec<String>,
    values: Vec<Option<String>>,
}

impl Row {
    pub fn from_data_row(columns: &[String], body: &[u8]) -> Self {
        let count = if body.len() >= 2 {
            i16::from_be_bytes(body[0..2].try_into().unwrap()) as usize
        } else {
            0
        };
        let mut values = Vec::with_capacity(count);
        let mut pos = 2;
        for _ in 0..count {
            if pos + 4 > body.len() {
                values.push(None);
                continue;
            }
            let len = i32::from_be_bytes(body[pos..pos + 4].try_into().unwrap());
            pos += 4;
            if len < 0 {
                values.push(None);
            } else {
                let len = len as usize;
                let s = std::str::from_utf8(&body[pos..pos + len])
                    .unwrap_or("")
                    .to_string();
                values.push(Some(s));
                pos += len;
            }
        }
        Self {
            columns: columns.to_vec(),
            values,
        }
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn try_get<'a, I, T>(&'a self, idx: I) -> Result<T, Error>
    where
        I: RowIndex,
        T: FromSql<'a>,
    {
        let i = idx.index(self)?;
        T::from_sql_nullable(self.values.get(i).and_then(|v| v.as_deref()))
    }

    pub fn get<'a, I, T>(&'a self, idx: I) -> T
    where
        I: RowIndex,
        T: FromSql<'a>,
    {
        self.try_get(idx).expect("row get")
    }
}

pub trait RowIndex {
    fn index(&self, row: &Row) -> Result<usize, Error>;
}

impl RowIndex for usize {
    fn index(&self, row: &Row) -> Result<usize, Error> {
        if *self >= row.len() {
            return Err(Error::msg("column index out of bounds"));
        }
        Ok(*self)
    }
}

impl RowIndex for str {
    fn index(&self, row: &Row) -> Result<usize, Error> {
        row.columns
            .iter()
            .position(|c| c == self)
            .ok_or_else(|| Error::msg(format!("column \"{self}\" not found")))
    }
}

impl RowIndex for &str {
    fn index(&self, row: &Row) -> Result<usize, Error> {
        (*self).index(row)
    }
}

impl RowIndex for String {
    fn index(&self, row: &Row) -> Result<usize, Error> {
        self.as_str().index(row)
    }
}

impl RowIndex for &String {
    fn index(&self, row: &Row) -> Result<usize, Error> {
        self.as_str().index(row)
    }
}
