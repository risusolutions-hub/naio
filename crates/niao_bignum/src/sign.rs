#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sign {
    Minus,
    NoSign,
    Plus,
}

impl Sign {
    #[inline]
    pub fn flip(self) -> Self {
        match self {
            Sign::Minus => Sign::Plus,
            Sign::Plus => Sign::Minus,
            Sign::NoSign => Sign::NoSign,
        }
    }

    #[inline]
    pub fn is_negative(self) -> bool {
        matches!(self, Sign::Minus)
    }
}
