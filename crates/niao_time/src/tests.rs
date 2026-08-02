use super::*;
use crate::civil::{days_from_civil, days_in_month, is_leap_year, is_valid_date};
use crate::tz::Timezone;
use crate::unix::civil_to_ms;

const FIX_2026_UTC: i64 = 1783080000000;
const FIX_NY_SPRING: i64 = 1710054000000;
const FIX_NY_FALL: i64 = 1730613600000;

#[test]
fn leap_years() {
    assert!(is_leap_year(2024));
    assert!(!is_leap_year(2023));
    assert!(is_leap_year(2000));
    assert!(!is_leap_year(1900));
    assert_eq!(days_in_month(2024, 2), 29);
    assert_eq!(days_in_month(2023, 2), 28);
    assert!(is_valid_date(2024, 2, 29));
    assert!(!is_valid_date(2023, 2, 29));
}

#[test]
fn civil_roundtrip_days() {
    let d = days_from_civil(2026, 7, 3);
    let (y, m, day) = crate::civil::civil_from_days(d);
    assert_eq!((y, m, day), (2026, 7, 3));
}

#[test]
fn utc_format_parse_roundtrip() {
    let tz = Timezone::utc();
    let dt = DateTime::from_unix_ms(FIX_2026_UTC);
    let s = dt.format("%Y-%m-%d %H:%M:%S", &tz).unwrap();
    assert_eq!(s, "2026-07-03 12:00:00");
    let parsed = DateTime::parse(&s, "%Y-%m-%d %H:%M:%S", &tz).unwrap();
    assert_eq!(parsed.unix_ms(), FIX_2026_UTC);
}

#[test]
fn kolkata_half_hour_offset() {
    let tz = Timezone::named("Asia/Kolkata").unwrap();
    let dt = DateTime::from_unix_ms(FIX_2026_UTC);
    assert_eq!(tz.offset_at_ms(FIX_2026_UTC).unwrap(), 19800);
    let civil = dt.to_civil(&tz).unwrap();
    assert_eq!(civil.hour, 17);
    assert_eq!(civil.minute, 30);
}

#[test]
fn new_york_dst_spring() {
    let tz = Timezone::named("America/New_York").unwrap();
    assert_eq!(tz.offset_at_ms(FIX_NY_SPRING - 1000).unwrap(), -18000);
    assert_eq!(tz.offset_at_ms(FIX_NY_SPRING).unwrap(), -14400);
    let civil = DateTime::from_unix_ms(FIX_NY_SPRING).to_civil(&tz).unwrap();
    assert_eq!(civil.hour, 3);
    assert_eq!(civil.minute, 0);
}

#[test]
fn new_york_dst_fall() {
    let tz = Timezone::named("America/New_York").unwrap();
    assert_eq!(tz.offset_at_ms(FIX_NY_FALL - 1000).unwrap(), -14400);
    assert_eq!(tz.offset_at_ms(FIX_NY_FALL).unwrap(), -18000);
    let civil = DateTime::from_unix_ms(FIX_NY_FALL).to_civil(&tz).unwrap();
    assert_eq!(civil.hour, 1);
}

#[test]
fn lord_howe_half_hour_dst() {
    let tz = Timezone::named("Australia/Lord_Howe").unwrap();
    assert_eq!(tz.offset_at_ms(FIX_2026_UTC).unwrap(), 37800);
    let civil = DateTime::from_unix_ms(FIX_2026_UTC).to_civil(&tz).unwrap();
    assert_eq!(civil.hour, 22);
    assert_eq!(civil.minute, 30);
}

#[test]
fn from_parts_edt() {
    let tz = Timezone::named("America/New_York").unwrap();
    let civil = crate::civil::CivilDateTime {
        year: 2026,
        month: 7,
        day: 3,
        hour: 8,
        minute: 0,
        second: 0,
        millisecond: 0,
    };
    let ms = civil_to_ms(&civil, &tz).unwrap();
    assert_eq!(ms, FIX_2026_UTC);
}

#[test]
fn rfc3339_utc() {
    let (civil, off) = parse_rfc3339("2026-07-03T12:00:00.000Z").unwrap();
    assert_eq!(off, 0);
    let ms = civil_to_ms(&civil, &Timezone::utc()).unwrap();
    assert_eq!(ms, FIX_2026_UTC);
}

#[test]
fn list_zones_nonempty() {
    assert!(list_timezones().len() >= 4);
}
