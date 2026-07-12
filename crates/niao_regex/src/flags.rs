#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub case_insensitive: bool,
    pub multiline: bool,
    pub dot_all: bool,
}

impl Flags {
    pub fn apply_inline(&mut self, s: &str) -> usize {
        let mut i = 0;
        let bytes = s.as_bytes();
        while i < bytes.len() {
            match bytes[i] {
                b'i' => self.case_insensitive = true,
                b'm' => self.multiline = true,
                b's' => self.dot_all = true,
                b'u' | b'U' => {}
                b'-' => {
                    i += 1;
                    while i < bytes.len() {
                        match bytes[i] {
                            b'i' => self.case_insensitive = false,
                            b'm' => self.multiline = false,
                            b's' => self.dot_all = false,
                            b'u' | b'U' => {}
                            _ => break,
                        }
                        i += 1;
                    }
                    break;
                }
                _ => break,
            }
            i += 1;
        }
        i
    }
}
