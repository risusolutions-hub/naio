mod transitions;

use transitions::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TzKind {
    Utc,
    Local,
    Named(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timezone {
    kind: TzKind,
}

impl Timezone {
    pub fn utc() -> Self {
        Self {
            kind: TzKind::Utc,
        }
    }

    pub fn local() -> Self {
        Self {
            kind: TzKind::Local,
        }
    }

    pub fn named(name: &str) -> Result<Self, String> {
        let n = name.trim();
        let lower = n.to_ascii_lowercase();
        match lower.as_str() {
            "utc" | "gmt" | "z" => Ok(Self::utc()),
            "local" => Ok(Self::local()),
            _ => {
                if lookup_transitions(n).is_some() {
                    Ok(Self {
                        kind: TzKind::Named(leak_name(n)),
                    })
                } else {
                    Err(format!("unknown timezone '{n}'"))
                }
            }
        }
    }

    pub fn name(&self) -> &'static str {
        match self.kind {
            TzKind::Utc => "UTC",
            TzKind::Local => "local",
            TzKind::Named(n) => n,
        }
    }

    pub fn offset_at_ms(&self, unix_ms: i64) -> Result<i32, String> {
        let secs = unix_ms.div_euclid(1000);
        Ok(self.offset_at_secs(secs))
    }

    pub fn offset_at_secs(&self, unix_secs: i64) -> i32 {
        match self.kind {
            TzKind::Utc => 0,
            TzKind::Local => crate::local::local_offset_secs(unix_secs),
            TzKind::Named(name) => offset_from_table(name, unix_secs),
        }
    }
}

fn leak_name(s: &str) -> &'static str {
    let names: &[&str] = &[
        "UTC",
        "America/New_York",
        "America/Chicago",
        "America/Denver",
        "America/Los_Angeles",
        "Europe/London",
        "Europe/Paris",
        "Asia/Kolkata",
        "Asia/Tokyo",
        "Australia/Sydney",
        "Australia/Lord_Howe",
        "Pacific/Auckland",
    ];
    names
        .iter()
        .find(|n| **n == s)
        .copied()
        .unwrap_or("UTC")
}

pub fn resolve_timezone(name: &str) -> Result<Timezone, String> {
    Timezone::named(name)
}

pub fn list_timezones() -> &'static [&'static str] {
    &[
        "UTC",
        "America/New_York",
        "America/Chicago",
        "America/Denver",
        "America/Los_Angeles",
        "Europe/London",
        "Europe/Paris",
        "Asia/Kolkata",
        "Asia/Tokyo",
        "Australia/Sydney",
        "Australia/Lord_Howe",
        "Pacific/Auckland",
    ]
}

fn lookup_transitions(name: &str) -> Option<&'static [(i64, i32)]> {
    match name {
        "UTC" => Some(TRANS_UTC),
        "America/New_York" => Some(TRANS_AMERICA_NEW_YORK),
        "America/Chicago" => Some(TRANS_AMERICA_CHICAGO),
        "America/Denver" => Some(TRANS_AMERICA_DENVER),
        "America/Los_Angeles" => Some(TRANS_AMERICA_LOS_ANGELES),
        "Europe/London" => Some(TRANS_EUROPE_LONDON),
        "Europe/Paris" => Some(TRANS_EUROPE_PARIS),
        "Asia/Kolkata" => Some(TRANS_ASIA_KOLKATA),
        "Asia/Tokyo" => Some(TRANS_ASIA_TOKYO),
        "Australia/Sydney" => Some(TRANS_AUSTRALIA_SYDNEY),
        "Australia/Lord_Howe" => Some(TRANS_AUSTRALIA_LORD_HOWE),
        "Pacific/Auckland" => Some(TRANS_PACIFIC_AUCKLAND),
        _ => None,
    }
}

fn offset_from_table(name: &str, unix_secs: i64) -> i32 {
    let Some(table) = lookup_transitions(name) else {
        return 0;
    };
    let mut off = table.first().map(|&(_, o)| o).unwrap_or(0);
    for &(ts, o) in table {
        if ts <= unix_secs {
            off = o;
        } else {
            break;
        }
    }
    off
}

/// Convert UTC unix ms to local civil time in `tz`.
pub fn utc_ms_to_civil(unix_ms: i64, tz: &Timezone) -> Result<crate::civil::CivilDateTime, String> {
    crate::unix::ms_to_civil(unix_ms, tz)
}

/// Interpret civil local time in `tz` as UTC unix ms (unique instant only).
pub fn civil_to_utc_ms(civil: &crate::civil::CivilDateTime, tz: &Timezone) -> Result<i64, String> {
    crate::unix::civil_to_ms(civil, tz)
}
