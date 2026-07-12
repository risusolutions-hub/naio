const METACHAR: &[u8] = b".^$*+?{}[]\\|()";

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for b in text.bytes() {
        if METACHAR.contains(&b) {
            out.push('\\');
        }
        out.push(b as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_dots() {
        assert_eq!(escape("a.b"), r"a\.b");
    }
}
