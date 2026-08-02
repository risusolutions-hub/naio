use crate::civil::{
    days_from_civil, weekday_from_days, CivilDateTime, MONTH_ABBR, MONTH_NAMES, WEEKDAY_ABBR,
    WEEKDAY_NAMES,
};

pub fn format_datetime(
    civil: &CivilDateTime,
    fmt: &str,
    offset_secs: i32,
) -> Result<String, String> {
    let mut out = String::with_capacity(fmt.len() + 16);
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            i += 1;
            if i >= bytes.len() {
                out.push('%');
                break;
            }
            if bytes[i] == b'.' {
                i += 1;
                if i < bytes.len() && bytes[i] == b'3' {
                    i += 1;
                    if i < bytes.len() && bytes[i] == b'f' {
                        i += 1;
                        write3(&mut out, civil.millisecond);
                        continue;
                    }
                }
                out.push('.');
                continue;
            }
            if bytes[i] == b':' {
                i += 1;
                if i < bytes.len() && bytes[i] == b'z' {
                    i += 1;
                    write_offset_colon(&mut out, offset_secs);
                    continue;
                }
                out.push(':');
                continue;
            }
            match bytes[i] as char {
                'Y' => write4(&mut out, civil.year),
                'm' => write2(&mut out, civil.month),
                'd' => write2(&mut out, civil.day),
                'H' => write2(&mut out, civil.hour),
                'M' => write2(&mut out, civil.minute),
                'S' => write2(&mut out, civil.second),
                'z' => write_offset(&mut out, offset_secs),
                'a' => {
                    let wd = weekday_from_days(days_from_civil(civil.year, civil.month, civil.day));
                    out.push_str(WEEKDAY_ABBR[wd]);
                }
                'A' => {
                    let wd = weekday_from_days(days_from_civil(civil.year, civil.month, civil.day));
                    out.push_str(WEEKDAY_NAMES[wd]);
                }
                'b' => {
                    if civil.month >= 1 && civil.month <= 12 {
                        out.push_str(MONTH_ABBR[(civil.month - 1) as usize]);
                    }
                }
                'B' => {
                    if civil.month >= 1 && civil.month <= 12 {
                        out.push_str(MONTH_NAMES[(civil.month - 1) as usize]);
                    }
                }
                c => out.push(c),
            }
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(out)
}

fn write2(out: &mut String, n: u32) {
    out.push(char::from_u32(n / 10 + b'0' as u32).unwrap_or('0'));
    out.push(char::from_u32(n % 10 + b'0' as u32).unwrap_or('0'));
}

fn write3(out: &mut String, n: u32) {
    out.push(char::from_u32(n / 100 + b'0' as u32).unwrap_or('0'));
    out.push(char::from_u32((n / 10) % 10 + b'0' as u32).unwrap_or('0'));
    out.push(char::from_u32(n % 10 + b'0' as u32).unwrap_or('0'));
}

fn write4(out: &mut String, n: i32) {
    let n = n.max(0) as u32;
    out.push(char::from_u32(n / 1000 + b'0' as u32).unwrap_or('0'));
    out.push(char::from_u32((n / 100) % 10 + b'0' as u32).unwrap_or('0'));
    out.push(char::from_u32((n / 10) % 10 + b'0' as u32).unwrap_or('0'));
    out.push(char::from_u32(n % 10 + b'0' as u32).unwrap_or('0'));
}

fn write_offset(out: &mut String, off: i32) {
    if off >= 0 {
        out.push('+');
    } else {
        out.push('-');
    }
    let abs = off.abs();
    let h = abs / 3600;
    let m = (abs % 3600) / 60;
    write2(out, h as u32);
    write2(out, m as u32);
}

fn write_offset_colon(out: &mut String, off: i32) {
    if off >= 0 {
        out.push('+');
    } else {
        out.push('-');
    }
    let abs = off.abs();
    let h = abs / 3600;
    let m = (abs % 3600) / 60;
    write2(out, h as u32);
    out.push(':');
    write2(out, m as u32);
}
