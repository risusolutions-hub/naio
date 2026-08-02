//! Endianness markers matching Python `struct` prefixes.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endian {
    /// `@` — native endian with alignment.
    Native,
    /// `=` — native endian, standard sizes, no alignment padding.
    NativeStandard,
    /// `<` — little-endian, no alignment.
    Little,
    /// `>` / `!` — big-endian (network), with alignment.
    Big,
}

impl Endian {
    pub fn is_little(self) -> bool {
        match self {
            Endian::Little => true,
            Endian::Native | Endian::NativeStandard => cfg!(target_endian = "little"),
            Endian::Big => false,
        }
    }

    pub fn uses_alignment(self) -> bool {
        matches!(self, Endian::Native | Endian::Big)
    }

    pub fn marker(self) -> &'static str {
        match self {
            Endian::Native => "@",
            Endian::NativeStandard => "=",
            Endian::Little => "<",
            Endian::Big => ">",
        }
    }
}
