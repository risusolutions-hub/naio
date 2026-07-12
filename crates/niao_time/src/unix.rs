use crate::civil::{civil_from_days, days_from_civil, weekday_from_days, CivilDateTime};
use crate::tz::Timezone;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UtcParts {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millisecond: u32,
    pub weekday: usize,
}

pub fn ms_to_utc_parts(ms: i64) -> Option<UtcParts> {
    if ms < 0 {
        // support pre-epoch with div_euclid
    }
    let secs = ms.div_euclid(1000);
    let sub_ms = ms.rem_euclid(1000) as u32;
    let days = secs.div_euclid(86_400);
    let sod = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days as i32);
    Some(UtcParts {
        year: y,
        month: m,
        day: d,
        hour: (sod / 3600) as u32,
        minute: ((sod % 3600) / 60) as u32,
        second: (sod % 60) as u32,
        millisecond: sub_ms,
        weekday: weekday_from_days(days as i32),
    })
}

pub fn utc_parts_to_ms(p: &UtcParts) -> Option<i64> {
    if !crate::civil::is_valid_date(p.year, p.month, p.day) {
        return None;
    }
    let days = days_from_civil(p.year, p.month, p.day) as i64;
    let secs = days * 86_400
        + p.hour as i64 * 3600
        + p.minute as i64 * 60
        + p.second as i64;
    Some(secs * 1000 + p.millisecond as i64)
}

pub fn ms_to_civil(ms: i64, tz: &Timezone) -> Result<CivilDateTime, String> {
    let offset = tz.offset_at_ms(ms)?;
    let local_ms = ms + offset as i64 * 1000;
    let p = ms_to_utc_parts(local_ms).ok_or_else(|| format!("invalid unix timestamp: {ms}"))?;
    Ok(CivilDateTime {
        year: p.year,
        month: p.month,
        day: p.day,
        hour: p.hour,
        minute: p.minute,
        second: p.second,
        millisecond: p.millisecond,
    })
}

pub fn civil_to_ms(civil: &CivilDateTime, tz: &Timezone) -> Result<i64, String> {
    if !crate::civil::is_valid_date(civil.year, civil.month, civil.day) {
        return Err("invalid date".into());
    }
    let days = days_from_civil(civil.year, civil.month, civil.day) as i64;
    let local_secs = days * 86_400
        + civil.hour as i64 * 3600
        + civil.minute as i64 * 60
        + civil.second as i64;
    let local_ms = local_secs * 1000 + civil.millisecond as i64;

    // UTC = local - offset; offset depends on instant — iterate once for DST edges.
    let mut utc_ms = local_ms - tz.offset_at_ms(local_ms)? as i64 * 1000;
    let offset = tz.offset_at_ms(utc_ms)?;
    utc_ms = local_ms - offset as i64 * 1000;
    Ok(utc_ms)
}
