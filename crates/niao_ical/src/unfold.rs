/// Unfold RFC 5545 continuation lines into logical lines.
///
/// >>> use niao_ical::unfold::unfold_lines;
/// >>> let lines: Vec<_> = unfold_lines("SUMMARY:Long\r\n line").collect();
/// >>> lines[0] == "SUMMARY:Longline"
/// true
pub fn unfold_lines(input: &str) -> UnfoldLines<'_> {
    UnfoldLines {
        input,
        pos: 0,
        pending: None,
    }
}

pub struct UnfoldLines<'a> {
    input: &'a str,
    pos: usize,
    pending: Option<String>,
}

impl<'a> Iterator for UnfoldLines<'a> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(p) = self.pending.take() {
            return Some(p);
        }
        if self.pos >= self.input.len() {
            return None;
        }
        let rest = &self.input[self.pos..];
        let (line, consumed) = take_physical_line(rest);
        self.pos += consumed;
        let mut logical = line.trim_end_matches(['\r', '\n']).to_string();
        while self.pos < self.input.len() {
            let rest = &self.input[self.pos..];
            if rest.starts_with(' ') || rest.starts_with('\t') {
                let (cont, consumed) = take_physical_line(rest);
                self.pos += consumed;
                let trimmed = cont.trim_end_matches(['\r', '\n']);
                logical.push_str(trimmed.trim_start_matches([' ', '\t']));
            } else {
                break;
            }
        }
        if logical.is_empty() {
            return self.next();
        }
        Some(logical)
    }
}

fn take_physical_line(s: &str) -> (&str, usize) {
    match s.find(['\n', '\r']) {
        Some(i) => {
            let mut end = i + 1;
            if s.as_bytes().get(i) == Some(&b'\r') && s.as_bytes().get(i + 1) == Some(&b'\n') {
                end = i + 2;
            }
            (&s[..end], end)
        }
        None => (s, s.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds() {
        let src = "SUMMARY:Long\r\n line\r\nUID:1\r\n";
        let lines: Vec<_> = unfold_lines(src).collect();
        assert_eq!(lines[0], "SUMMARY:Longline");
        assert_eq!(lines[1], "UID:1");
    }
}
