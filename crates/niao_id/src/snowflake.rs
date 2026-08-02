//! Twitter-style 64-bit snowflake IDs (timestamp + datacenter + worker + sequence).

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_EPOCH_MS: u64 = 1_288_834_974_657; // Twitter snowflake epoch
pub const MAX_WORKER_ID: u16 = 31;
pub const MAX_DATACENTER_ID: u16 = 31;

const WORKER_BITS: u64 = 5;
const DATACENTER_BITS: u64 = 5;
const SEQUENCE_BITS: u64 = 12;
const WORKER_SHIFT: u64 = SEQUENCE_BITS;
const DATACENTER_SHIFT: u64 = SEQUENCE_BITS + WORKER_BITS;
const TIMESTAMP_SHIFT: u64 = SEQUENCE_BITS + WORKER_BITS + DATACENTER_BITS;
const SEQUENCE_MASK: u64 = (1 << SEQUENCE_BITS) - 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnowflakeError {
    WorkerIdOutOfRange,
    DatacenterIdOutOfRange,
    ClockMovedBackwards,
}

impl std::fmt::Display for SnowflakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkerIdOutOfRange => write!(f, "worker_id must be 0..={MAX_WORKER_ID}"),
            Self::DatacenterIdOutOfRange => {
                write!(f, "datacenter_id must be 0..={MAX_DATACENTER_ID}")
            }
            Self::ClockMovedBackwards => write!(f, "system clock moved backwards"),
        }
    }
}

impl std::error::Error for SnowflakeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnowflakeParts {
    pub timestamp_ms: u64,
    pub datacenter_id: u16,
    pub worker_id: u16,
    pub sequence: u16,
}

/// Thread-safe snowflake generator.
pub struct SnowflakeGenerator {
    epoch_ms: u64,
    worker_id: u64,
    datacenter_id: u64,
    state: AtomicU64, // packed: last_ts << 22 | seq
}

impl SnowflakeGenerator {
    pub fn new(worker_id: u16, datacenter_id: u16) -> Result<Self, SnowflakeError> {
        Self::with_epoch(worker_id, datacenter_id, DEFAULT_EPOCH_MS)
    }

    pub fn with_epoch(
        worker_id: u16,
        datacenter_id: u16,
        epoch_ms: u64,
    ) -> Result<Self, SnowflakeError> {
        if worker_id > MAX_WORKER_ID {
            return Err(SnowflakeError::WorkerIdOutOfRange);
        }
        if datacenter_id > MAX_DATACENTER_ID {
            return Err(SnowflakeError::DatacenterIdOutOfRange);
        }
        Ok(Self {
            epoch_ms,
            worker_id: worker_id as u64,
            datacenter_id: datacenter_id as u64,
            state: AtomicU64::new(0),
        })
    }

    pub fn next_id(&self) -> Result<i64, SnowflakeError> {
        loop {
            let packed = self.state.load(Ordering::Relaxed);
            let last_ts = packed >> 22;
            let last_seq = packed & SEQUENCE_MASK;

            let now = now_ms();
            if now < last_ts {
                return Err(SnowflakeError::ClockMovedBackwards);
            }

            let (ts, seq) = if now == last_ts {
                let next_seq = (last_seq + 1) & SEQUENCE_MASK;
                if next_seq == 0 {
                    // spin until next millisecond
                    std::hint::spin_loop();
                    continue;
                }
                (now, next_seq)
            } else {
                (now, 0)
            };

            let new_packed = (ts << 22) | seq;
            if self
                .state
                .compare_exchange_weak(packed, new_packed, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                let id = ((ts - self.epoch_ms) << TIMESTAMP_SHIFT)
                    | (self.datacenter_id << DATACENTER_SHIFT)
                    | (self.worker_id << WORKER_SHIFT)
                    | seq;
                return Ok(id as i64);
            }
        }
    }

    pub fn epoch_ms(&self) -> u64 {
        self.epoch_ms
    }

    pub fn worker_id(&self) -> u16 {
        self.worker_id as u16
    }

    pub fn datacenter_id(&self) -> u16 {
        self.datacenter_id as u16
    }
}

pub fn parse(id: i64, epoch_ms: u64) -> SnowflakeParts {
    let u = id as u64;
    let sequence = (u & SEQUENCE_MASK) as u16;
    let worker_id = ((u >> WORKER_SHIFT) & ((1 << WORKER_BITS) - 1)) as u16;
    let datacenter_id = ((u >> DATACENTER_SHIFT) & ((1 << DATACENTER_BITS) - 1)) as u16;
    let timestamp_ms = (u >> TIMESTAMP_SHIFT) + epoch_ms;
    SnowflakeParts {
        timestamp_ms,
        datacenter_id,
        worker_id,
        sequence,
    }
}

#[inline]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_and_parse_roundtrip() {
        let gen = SnowflakeGenerator::new(3, 7).unwrap();
        let a = gen.next_id().unwrap();
        let b = gen.next_id().unwrap();
        assert_ne!(a, b);
        let parts = parse(a, DEFAULT_EPOCH_MS);
        assert_eq!(parts.worker_id, 3);
        assert_eq!(parts.datacenter_id, 7);
    }
}
