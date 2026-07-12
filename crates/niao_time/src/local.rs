//! Local timezone offset via `TZ` env or OS APIs (no third-party crates).

#[cfg(windows)]
pub fn local_offset_secs(unix_secs: i64) -> i32 {
    if let Ok(tz) = std::env::var("TZ") {
        if let Ok(z) = crate::tz::Timezone::named(&tz) {
            return z.offset_at_secs(unix_secs);
        }
    }
    windows_offset_secs()
}

#[cfg(not(windows))]
pub fn local_offset_secs(unix_secs: i64) -> i32 {
    if let Ok(tz) = std::env::var("TZ") {
        if let Ok(z) = crate::tz::Timezone::named(&tz) {
            return z.offset_at_secs(unix_secs);
        }
    }
    unix_offset_secs()
}

#[cfg(windows)]
fn windows_offset_secs() -> i32 {
    use std::mem::MaybeUninit;

    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    extern "system" {
        fn GetLocalTime(st: *mut SystemTime);
        fn GetSystemTime(st: *mut SystemTime);
    }

    unsafe {
        let mut local = MaybeUninit::<SystemTime>::uninit();
        let mut utc = MaybeUninit::<SystemTime>::uninit();
        GetLocalTime(local.as_mut_ptr());
        GetSystemTime(utc.as_mut_ptr());
        let l = local.assume_init();
        let u = utc.assume_init();
        let local_secs = (l.hour as i32) * 3600 + (l.minute as i32) * 60 + l.second as i32;
        let utc_secs = (u.hour as i32) * 3600 + (u.minute as i32) * 60 + u.second as i32;
        let mut diff = local_secs - utc_secs;
        if (l.day as i32) != (u.day as i32) {
            if l.day > u.day || (l.day == 1 && u.day > 20) {
                diff += 86_400;
            } else {
                diff -= 86_400;
            }
        }
        diff
    }
}

#[cfg(not(windows))]
fn unix_offset_secs() -> i32 {
    use std::mem::MaybeUninit;

    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const u8,
    }

    extern "C" {
        fn time(t: *mut i64) -> i64;
        fn localtime_r(t: *const i64, tm: *mut Tm) -> *mut Tm;
        fn gmtime_r(t: *const i64, tm: *mut Tm) -> *mut Tm;
    }

    unsafe {
        let mut t = 0i64;
        time(&mut t);
        let mut lt = MaybeUninit::<Tm>::uninit();
        let mut gt = MaybeUninit::<Tm>::uninit();
        if localtime_r(&t, lt.as_mut_ptr()).is_null() || gmtime_r(&t, gt.as_mut_ptr()).is_null() {
            return 0;
        }
        let lt = lt.assume_init();
        let gt = gt.assume_init();
        let local_secs = lt.tm_hour * 3600 + lt.tm_min * 60 + lt.tm_sec;
        let utc_secs = gt.tm_hour * 3600 + gt.tm_min * 60 + gt.tm_sec;
        let mut diff = local_secs - utc_secs;
        if lt.tm_mday != gt.tm_mday {
            if lt.tm_mday > gt.tm_mday {
                diff += 86_400;
            } else {
                diff -= 86_400;
            }
        }
        diff
    }
}
