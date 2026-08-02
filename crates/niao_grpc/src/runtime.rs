//! Shared Tokio runtime for sync Niao↔async h2 boundary.

use std::sync::OnceLock;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("ngrpc")
            .worker_threads(2)
            .build()
            .expect("failed to create ngrpc tokio runtime")
    })
}

pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    runtime().block_on(f)
}
