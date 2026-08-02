//! `niao_browser` — headless browser automation via CDP:
//! navigate, click, fill, screenshot, PDF (~playwright, selenium).
//!
//! Thin Niao binding lives in `niao_runtime::nbrowser`; this crate holds the
//! protocol logic so a future C11 port only needs a new boundary layer.

mod cdp;
mod config;
mod error;
mod runtime;
mod session;
mod util;

pub use config::{
    ConnectConfig, CookieInput, ImageFormat, LaunchConfig, NavOpts, PdfOpts, ScreenshotOpts,
    Viewport, WaitUntil,
};
pub use error::{BrowserError, BrowserResult};
pub use session::{
    attr, check, clear_cookies, click, close, close_page, connect, content, cookies, count, eval,
    exists, fill, focus, goto, hover, is_connected, launch, new_page, page_alive, pages, pdf,
    press, reload, screenshot, select_option, set_cookie, set_extra_headers, set_viewport,
    text_content, title, type_text, uncheck, url, version, wait_for, CookieInfo, GotoResult,
};
pub use util::{
    executable_path, js_string_literal, require_selector, require_url, resolve_executable,
};
