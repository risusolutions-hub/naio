//! Sorted containers for Niao — list, set, dict with bisect and range queries.

mod bisect;
mod dict;
mod key;
mod list;
mod set;

pub use bisect::{
    bisect_left_int, bisect_right_int, insort_int, nearest_int, nearest_left_int, nearest_right_int,
};
pub use dict::SortedDict;
pub use key::{ListSlot, SetKey, SortValue};
pub use list::SortedList;
pub use set::SortedSet;

/// Unified error type surfaced by container operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortError {
    IncompatibleTypes,
    NotFound,
    IndexOutOfBounds,
    Empty,
}

impl From<list::SortError> for SortError {
    fn from(e: list::SortError) -> Self {
        match e {
            list::SortError::IncompatibleTypes => SortError::IncompatibleTypes,
            list::SortError::NotFound => SortError::NotFound,
            list::SortError::IndexOutOfBounds => SortError::IndexOutOfBounds,
            list::SortError::Empty => SortError::Empty,
        }
    }
}

impl From<set::SortError> for SortError {
    fn from(e: set::SortError) -> Self {
        match e {
            set::SortError::IncompatibleTypes => SortError::IncompatibleTypes,
            set::SortError::NotFound => SortError::NotFound,
            set::SortError::IndexOutOfBounds => SortError::IndexOutOfBounds,
            set::SortError::Empty => SortError::Empty,
        }
    }
}

impl From<dict::SortError> for SortError {
    fn from(e: dict::SortError) -> Self {
        match e {
            dict::SortError::IncompatibleTypes => SortError::IncompatibleTypes,
            dict::SortError::NotFound => SortError::NotFound,
            dict::SortError::IndexOutOfBounds => SortError::IndexOutOfBounds,
            dict::SortError::Empty => SortError::Empty,
        }
    }
}
