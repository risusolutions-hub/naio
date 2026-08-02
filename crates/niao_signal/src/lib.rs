//! `niao_signal` — cross-platform OS signal registration, delivery queue, and
//! helpers used by the Niao `nsignal` standard library.
//!
//! Delivery is deferred: signal handlers only enqueue the signal number; callers
//! poll from normal code (async-signal-safe). Built on [`signal-hook`].

use signal_hook::consts::signal::*;
use std::collections::HashMap;
use std::io;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Sentinel returned by [`HandlerKind::as_i64`] for default / ignore slots.
pub const SIG_DFL_SENTINEL: i64 = -1;
pub const SIG_IGN_SENTINEL: i64 = -2;

/// How a signal is handled at the OS boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    /// OS default disposition; not watched by the engine.
    Default,
    /// Ignored at the OS level.
    Ignore,
    /// Watched and delivered to the poll queue for user handlers.
    Watched,
}

impl HandlerKind {
    #[inline]
    pub fn as_i64(self) -> i64 {
        match self {
            HandlerKind::Default => SIG_DFL_SENTINEL,
            HandlerKind::Ignore => SIG_IGN_SENTINEL,
            HandlerKind::Watched => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Platform signal tables
// ---------------------------------------------------------------------------

#[cfg(unix)]
const PLATFORM_SIGNALS: &[(i32, &str)] = &[
    (SIGINT, "sigint"),
    (SIGTERM, "sigterm"),
    (SIGHUP, "sighup"),
    (SIGABRT, "sigabrt"),
    (SIGPIPE, "sigpipe"),
    (SIGALRM, "sigalrm"),
    (SIGCHLD, "sigchld"),
    (SIGUSR1, "sigusr1"),
    (SIGUSR2, "sigusr2"),
    (SIGQUIT, "sigquit"),
    (SIGILL, "sigill"),
    (SIGFPE, "sigfpe"),
    (SIGSEGV, "sigsegv"),
    (SIGTSTP, "sigtstp"),
    (SIGCONT, "sigcont"),
    (SIGTTIN, "sigttin"),
    (SIGTTOU, "sigttou"),
    (SIGWINCH, "sigwinch"),
];

#[cfg(windows)]
const PLATFORM_SIGNALS: &[(i32, &str)] = &[
    (SIGINT, "sigint"),
    (SIGTERM, "sigterm"),
    (SIGABRT, "sigabrt"),
    (SIGFPE, "sigfpe"),
    (SIGILL, "sigill"),
    (SIGSEGV, "sigsegv"),
    (SIGBREAK, "sigbreak"),
];

// ---------------------------------------------------------------------------
// Engine — unix uses Signals iterator; windows uses atomic flags.
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod engine {
    use super::*;
    use libc::{signal, SIG_DFL, SIG_IGN};
    use signal_hook::iterator::Signals;

    pub struct EngineState {
        watch_list: Vec<i32>,
        kinds: HashMap<i32, HandlerKind>,
        signals: Option<Signals>,
    }

    impl EngineState {
        pub fn new() -> Self {
            Self {
                watch_list: Vec::new(),
                kinds: HashMap::new(),
                signals: None,
            }
        }

        fn rebuild(&mut self) -> io::Result<()> {
            self.watch_list = self
                .kinds
                .iter()
                .filter_map(|(&sig, kind)| (*kind == HandlerKind::Watched).then_some(sig))
                .collect();
            self.watch_list.sort_unstable();
            self.watch_list.dedup();
            self.signals = if self.watch_list.is_empty() {
                None
            } else {
                Some(Signals::new(&self.watch_list)?)
            };
            Ok(())
        }

        pub fn set_kind(&mut self, sig: i32, kind: HandlerKind) -> io::Result<()> {
            match kind {
                HandlerKind::Default => {
                    // SAFETY: restoring the platform default handler for a known signal number.
                    unsafe {
                        signal(sig, SIG_DFL);
                    }
                    self.kinds.remove(&sig);
                }
                HandlerKind::Ignore => {
                    // SAFETY: installing SIG_IGN for a known signal number.
                    unsafe {
                        signal(sig, SIG_IGN);
                    }
                    self.kinds.insert(sig, HandlerKind::Ignore);
                }
                HandlerKind::Watched => {
                    self.kinds.insert(sig, HandlerKind::Watched);
                }
            }
            self.rebuild()
        }

        pub fn kind(&self, sig: i32) -> HandlerKind {
            self.kinds
                .get(&sig)
                .copied()
                .unwrap_or(HandlerKind::Default)
        }

        pub fn peek_pending(&mut self) -> Vec<i32> {
            let Some(signals) = self.signals.as_mut() else {
                return Vec::new();
            };
            signals.pending().collect()
        }

        pub fn drain_pending(&mut self) -> Vec<i32> {
            self.peek_pending()
        }

        pub fn reset_all(&mut self) -> io::Result<()> {
            let keys: Vec<i32> = self.kinds.keys().copied().collect();
            for sig in keys {
                // SAFETY: restoring the platform default handler for a known signal number.
                unsafe {
                    signal(sig, SIG_DFL);
                }
            }
            self.kinds.clear();
            self.rebuild()
        }
    }
}

#[cfg(windows)]
mod engine {
    use super::*;
    use signal_hook::flag;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct FlagSlot {
        flag: Arc<AtomicBool>,
        registered: bool,
    }

    pub struct EngineState {
        kinds: HashMap<i32, HandlerKind>,
        flags: HashMap<i32, FlagSlot>,
        queue: Vec<i32>,
    }

    impl EngineState {
        pub fn new() -> Self {
            Self {
                kinds: HashMap::new(),
                flags: HashMap::new(),
                queue: Vec::new(),
            }
        }

        fn ensure_flag(&mut self, sig: i32) -> io::Result<Arc<AtomicBool>> {
            let slot = self.flags.entry(sig).or_insert_with(|| FlagSlot {
                flag: Arc::new(AtomicBool::new(false)),
                registered: false,
            });
            if !slot.registered {
                flag::register(sig, Arc::clone(&slot.flag))?;
                slot.registered = true;
            }
            Ok(Arc::clone(&slot.flag))
        }

        fn clear_flag(&mut self, sig: i32) {
            if let Some(slot) = self.flags.get_mut(&sig) {
                slot.registered = false;
                slot.flag.store(false, Ordering::SeqCst);
            }
        }

        pub fn set_kind(&mut self, sig: i32, kind: HandlerKind) -> io::Result<()> {
            match kind {
                HandlerKind::Default | HandlerKind::Ignore => {
                    self.clear_flag(sig);
                    if kind == HandlerKind::Ignore {
                        self.kinds.insert(sig, HandlerKind::Ignore);
                    } else {
                        self.kinds.remove(&sig);
                    }
                }
                HandlerKind::Watched => {
                    let flag = self.ensure_flag(sig)?;
                    flag.store(false, Ordering::SeqCst);
                    self.kinds.insert(sig, HandlerKind::Watched);
                }
            }
            Ok(())
        }

        pub fn kind(&self, sig: i32) -> HandlerKind {
            self.kinds
                .get(&sig)
                .copied()
                .unwrap_or(HandlerKind::Default)
        }

        fn collect_flags(&mut self) {
            for (&sig, kind) in self.kinds.iter() {
                if *kind != HandlerKind::Watched {
                    continue;
                }
                if let Some(slot) = self.flags.get(&sig) {
                    if slot.flag.swap(false, Ordering::SeqCst) {
                        self.queue.push(sig);
                    }
                }
            }
        }

        pub fn peek_pending(&mut self) -> Vec<i32> {
            self.collect_flags();
            self.queue.clone()
        }

        pub fn drain_pending(&mut self) -> Vec<i32> {
            self.collect_flags();
            std::mem::take(&mut self.queue)
        }

        pub fn reset_all(&mut self) -> io::Result<()> {
            self.kinds.clear();
            self.queue.clear();
            self.flags.clear();
            Ok(())
        }
    }
}

use engine::EngineState;

fn engine() -> &'static Mutex<EngineState> {
    static ENGINE: OnceLock<Mutex<EngineState>> = OnceLock::new();
    ENGINE.get_or_init(|| Mutex::new(EngineState::new()))
}

/// Register `sig` for deferred delivery (`Watched`), ignore, or OS default.
pub fn set_handler_kind(sig: i32, kind: HandlerKind) -> io::Result<()> {
    if !is_valid_signal(sig) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid signal number {sig}"),
        ));
    }
    engine().lock().unwrap().set_kind(sig, kind)
}

/// Current handler kind for `sig`.
pub fn handler_kind(sig: i32) -> HandlerKind {
    engine().lock().unwrap().kind(sig)
}

/// Non-blocking peek at pending watched signals (does not consume on unix).
pub fn peek_pending() -> Vec<i32> {
    engine().lock().unwrap().peek_pending()
}

/// Non-blocking drain of pending watched signals.
pub fn drain_pending() -> Vec<i32> {
    engine().lock().unwrap().drain_pending()
}

/// Block until `target` (or any signal when `None`) arrives, optional timeout.
pub fn wait_for(target: Option<i32>, timeout: Option<Duration>) -> Option<i32> {
    let deadline = timeout.map(|t| Instant::now() + t);
    loop {
        let pending = drain_pending();
        if let Some(sig) = target {
            if let Some(found) = pending.iter().copied().find(|&s| s == sig) {
                return Some(found);
            }
        } else if let Some(sig) = pending.into_iter().next() {
            return Some(sig);
        }

        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Restore every registered disposition to OS default.
pub fn reset_all() -> io::Result<()> {
    engine().lock().unwrap().reset_all()
}

/// Raise `sig` in the current process (platform permitting).
pub fn raise_signal(sig: i32) -> io::Result<()> {
    if !is_valid_signal(sig) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid signal number {sig}"),
        ));
    }
    signal_hook::low_level::raise(sig)
}

/// Unix `alarm(2)`; returns previous seconds. On non-Unix returns `0`.
#[cfg(unix)]
pub fn alarm(seconds: u32) -> u32 {
    unsafe { libc::alarm(seconds) }
}

#[cfg(not(unix))]
pub fn alarm(_seconds: u32) -> u32 {
    0
}

/// Whether `sig` is a known platform signal.
#[inline]
pub fn is_valid_signal(sig: i32) -> bool {
    signal_name(sig).is_some()
}

/// Stable lowercase name for a signal number, if known on this platform.
pub fn signal_name(sig: i32) -> Option<&'static str> {
    PLATFORM_SIGNALS
        .iter()
        .find(|(n, _)| *n == sig)
        .map(|(_, name)| *name)
}

/// Parse `SIGINT` / `sigint` / `int` style names to a signal number.
pub fn parse_signal_name(name: &str) -> Option<i32> {
    let key = name.trim().to_ascii_lowercase();
    let key = key.strip_prefix("sig").unwrap_or(&key);
    PLATFORM_SIGNALS
        .iter()
        .find(|(_, n)| {
            *n == key
                || n.strip_prefix("sig").unwrap_or(n) == key
                || match *n {
                    "sigchld" => key == "cld",
                    _ => false,
                }
        })
        .map(|(num, _)| *num)
}

/// Human-readable description (`"SIGINT (Interrupt)"` style).
pub fn strsignal(sig: i32) -> Option<String> {
    let label = match sig {
        x if x == SIGINT => "Interrupt",
        x if x == SIGTERM => "Termination",
        x if x == SIGABRT => "Abort",
        #[cfg(unix)]
        x if x == SIGHUP => "Hangup",
        #[cfg(unix)]
        x if x == SIGPIPE => "Broken pipe",
        #[cfg(unix)]
        x if x == SIGALRM => "Alarm clock",
        #[cfg(unix)]
        x if x == SIGCHLD => "Child status",
        #[cfg(unix)]
        x if x == SIGUSR1 => "User-defined 1",
        #[cfg(unix)]
        x if x == SIGUSR2 => "User-defined 2",
        #[cfg(unix)]
        x if x == SIGQUIT => "Quit",
        x if x == SIGILL => "Illegal instruction",
        x if x == SIGFPE => "Floating-point exception",
        x if x == SIGSEGV => "Segmentation fault",
        #[cfg(unix)]
        x if x == SIGTSTP => "Stop",
        #[cfg(unix)]
        x if x == SIGCONT => "Continue",
        #[cfg(unix)]
        x if x == SIGTTIN => "TTY input",
        #[cfg(unix)]
        x if x == SIGTTOU => "TTY output",
        #[cfg(unix)]
        x if x == SIGWINCH => "Window size change",
        #[cfg(windows)]
        x if x == SIGBREAK => "Ctrl-Break",
        _ => return None,
    };
    signal_name(sig).map(|n| format!("{} ({label})", n.to_ascii_uppercase()))
}

/// All signal numbers known on this platform (sorted).
pub fn valid_signals() -> Vec<i32> {
    PLATFORM_SIGNALS.iter().map(|(n, _)| *n).collect()
}

/// Platform constant values exposed to the VM.
pub fn signal_constants() -> Vec<(&'static str, i32)> {
    PLATFORM_SIGNALS
        .iter()
        .map(|(num, name)| (*name, *num))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_name_roundtrip() {
        for sig in valid_signals() {
            let name = signal_name(sig).unwrap();
            assert_eq!(parse_signal_name(name), Some(sig));
            assert_eq!(parse_signal_name(&name.to_ascii_uppercase()), Some(sig));
        }
    }

    #[test]
    fn watch_drain_does_not_panic() {
        let _ = set_handler_kind(SIGINT, HandlerKind::Watched);
        let _ = drain_pending();
        let _ = set_handler_kind(SIGINT, HandlerKind::Default);
    }
}
