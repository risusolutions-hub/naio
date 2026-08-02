//! Shared Tokio runtime for sync wrappers around async CDP I/O.

use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .thread_name("niao-browser")
            .build()
            .expect("failed to create niao_browser tokio runtime")
    })
}

/// Drive an async future to completion on the shared runtime.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    runtime().block_on(fut)
}
