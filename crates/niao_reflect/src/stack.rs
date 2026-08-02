//! Lightweight call-stack frames for `nreflect.stack()`.

use std::cell::RefCell;

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub name: String,
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

thread_local! {
    static STACK: RefCell<Vec<StackFrame>> = const { RefCell::new(Vec::new()) };
}

pub struct FrameGuard {
    pushed: bool,
}

impl FrameGuard {
    pub fn new(name: impl Into<String>, file: Option<String>, line: usize, col: usize) -> Self {
        STACK.with(|s| {
            s.borrow_mut().push(StackFrame {
                name: name.into(),
                file,
                line,
                col,
            });
        });
        Self { pushed: true }
    }
}

impl Drop for FrameGuard {
    fn drop(&mut self) {
        if self.pushed {
            STACK.with(|s| {
                s.borrow_mut().pop();
            });
        }
    }
}

#[inline]
pub fn push_frame(
    name: impl Into<String>,
    file: Option<String>,
    line: usize,
    col: usize,
) -> FrameGuard {
    FrameGuard::new(name, file, line, col)
}

pub fn stack_frames() -> Vec<StackFrame> {
    STACK.with(|s| s.borrow().clone())
}

pub fn current_frame() -> Option<StackFrame> {
    STACK.with(|s| s.borrow().last().cloned())
}
