use crate::civil::CivilDateTime;

pub fn parse_datetime(text: &str, fmt: &str) -> Result<CivilDateTime, String> {
    let mut year = 1970i32;
    let mut month = 1u32;
    let mut day = 1u32;
    let mut hour = 0u32;
    let mut minute = 0u32;
    let mut second = 0u32;
    let mut millisecond = 0u32;

    let tb = text.as_bytes();
    let fb = fmt.as_bytes();
    let mut ti = 0usize;
    let mut fi = 0usize;

    while fi < fb.len() {
        if fb[fi] == b'%' {
            fi += 1;
            if fi >= fb.len() {
                return Err("truncated format".into());
            }
            if fb[fi] == b'.' {
                fi += 1;
                if fi < fb.len() && fb[fi] == b'3' {
                    fi += 1;
                    if fi < fb.len() && fb[fi] == b'f' {
                        fi += 1;
                        millisecond = parse_digits(tb, &mut ti, 3)?;
                        continue;
                    }
                }
            }
            match fb[fi] as char {
                'Y' => year = parse_digits(tb, &mut ti, 4)? as i32,
                'm' => month = parse_digits(tb, &mut ti, 2)?,
                'd' => day = parse_digits(tb, &mut ti, 2)?,
                'H' => hour = parse_digits(tb, &mut ti, 2)?,
                'M' => minute = parse_digits(tb, &mut ti, 2)?,
                'S' => second = parse_digits(tb, &mut ti, 2)?,
                c => {
                    ti = match_literal(tb, ti, c)?;
                }
            }
            fi += 1;
        } else {
            ti = match_literal(tb, ti, fb[fi] as char)?;
            fi += 1;
        }
    }

    Ok(CivilDateTime {
        year,
        month,
        day,
        hour,
        minute,
        second,
        millisecond,
    })
}

fn match_literal(tb: &[u8], ti: usize, ch: char) -> Result<usize, String> {
    if ti >= tb.len() || tb[ti] as char != ch {
        return Err(format!("expected '{ch}' at byte {ti}"));
    }
    Ok(ti + 1)
}

fn parse_digits(tb: &[u8], ti: &mut usize, n: usize) -> Result<u32, String> {
    let mut val = 0u32;
    for _ in 0..n {
        if *ti >= tb.len() || !tb[*ti].is_ascii_digit() {
            return Err("expected digit".into());
        }
        val = val * 10 + (tb[*ti] - b'0') as u32;
        *ti += 1;
    }
    Ok(val)
}

pub fn parse_rfc3339(text: &str) -> Result<(CivilDateTime, i32), String> {
    if text.len() < 19 {
        return Err("short rfc3339".into());
    }
    let civil = parse_datetime(&text[..19], "%Y-%m-%dT%H:%M:%S")?;
    let rest = &text[19..];
    let (ms, off) = if rest.starts_with('.') {
        let frac = &rest[1..];
        let end = frac
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(frac.len());
        let digits = &frac[..end];
        let mut ms = 0u32;
        for (i, b) in digits.bytes().enumerate().take(3) {
            ms = ms * 10 + (b - b'0') as u32;
            if i == 0 && digits.len() == 1 {
                ms *= 100;
            } else if i == 1 && digits.len() == 2 {
                ms *= 10;
            }
        }
        let tail = &rest[1 + end..];
        (ms, parse_offset(tail)?)
    } else {
        (0, parse_offset(rest)?)
    };
    Ok((
        CivilDateTime {
            millisecond: ms,
            ..civil
        },
        off,
    ))
}

fn parse_offset(s: &str) -> Result<i32, String> {
    if s.is_empty() || s.starts_with('Z') || s.starts_with('z') {
        return Ok(0);
    }
    let sign = if s.starts_with('-') { -1 } else { 1 };
    let body = s.trim_start_matches(['+', '-']);
    let parts: Vec<_> = body.split(':').collect();
    let h: i32 = parts
        .first()
        .ok_or_else(|| "offset".to_string())?
        .parse()
        .map_err(|_| "offset hour".to_string())?;
    let m: i32 = if parts.len() > 1 {
        parts[1].parse().map_err(|_| "offset min".to_string())?
    } else if body.len() > 2 {
        body[2..].parse().unwrap_or(0)
    } else {
        0
    };
    Ok(sign * (h * 3600 + m * 60))
}

pub fn parse_rfc2822(text: &str) -> Result<CivilDateTime, String> {
    // Minimal: Wed, 03 Jul 2026 12:00:00 +0000
    let parts: Vec<_> = text.split_whitespace().collect();
    if parts.len() < 5 {
        return Err("invalid rfc2822".into());
    }
    let day: u32 = parts[1].trim_end_matches(',').parse().map_err(|_| "day")?;
    let month = month_from_abbr(parts[2])?;
    let year: i32 = parts[3].parse().map_err(|_| "year")?;
    let hms = parts[4];
    let civil = parse_datetime(
        &format!("{year:04}{month:02}{day:02}{hms}"),
        "%Y%m%d%H:%M:%S",
    )?;
    Ok(civil)
}

fn month_from_abbr(s: &str) -> Result<u32, String> {
    let m = match s {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return Err(format!("bad month '{s}'")),
    };
    Ok(m)
}
