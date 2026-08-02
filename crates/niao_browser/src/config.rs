//! Launch / connect configuration.

/// Options for [`crate::launch`].
#[derive(Debug, Clone)]
pub struct LaunchConfig {
    pub headless: bool,
    pub executable: Option<String>,
    pub width: u32,
    pub height: u32,
    pub timeout_ms: u64,
    pub no_sandbox: bool,
    pub args: Vec<String>,
    pub user_data_dir: Option<String>,
    pub ignore_https_errors: bool,
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            headless: true,
            executable: None,
            width: 1280,
            height: 720,
            timeout_ms: 30_000,
            no_sandbox: false,
            args: Vec::new(),
            user_data_dir: None,
            ignore_https_errors: true,
        }
    }
}

/// Options for [`crate::connect`].
#[derive(Debug, Clone)]
pub struct ConnectConfig {
    /// WebSocket DevTools URL, or `http://host:port` HTTP debugging endpoint.
    pub endpoint: String,
    pub timeout_ms: u64,
}

impl ConnectConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            timeout_ms: 30_000,
        }
    }
}

/// Options for navigation / waits.
#[derive(Debug, Clone)]
pub struct NavOpts {
    pub timeout_ms: u64,
    pub wait_until: WaitUntil,
}

impl Default for NavOpts {
    fn default() -> Self {
        Self {
            timeout_ms: 30_000,
            wait_until: WaitUntil::Load,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitUntil {
    Load,
    NetworkIdle,
    DomContentLoaded,
}

impl WaitUntil {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "load" => Some(Self::Load),
            "networkidle" | "network_idle" => Some(Self::NetworkIdle),
            "domcontentloaded" | "dom_content_loaded" => Some(Self::DomContentLoaded),
            _ => None,
        }
    }
}

/// Screenshot options.
#[derive(Debug, Clone)]
pub struct ScreenshotOpts {
    pub full_page: bool,
    pub format: ImageFormat,
    pub quality: Option<u32>,
}

impl Default for ScreenshotOpts {
    fn default() -> Self {
        Self {
            full_page: false,
            format: ImageFormat::Png,
            quality: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

impl ImageFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpeg" | "jpg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }
}

/// PDF print options.
#[derive(Debug, Clone)]
pub struct PdfOpts {
    pub landscape: bool,
    pub print_background: bool,
    pub scale: f64,
    pub paper_width: Option<f64>,
    pub paper_height: Option<f64>,
}

impl Default for PdfOpts {
    fn default() -> Self {
        Self {
            landscape: false,
            print_background: true,
            scale: 1.0,
            paper_width: None,
            paper_height: None,
        }
    }
}

/// Cookie to set on a page.
#[derive(Debug, Clone)]
pub struct CookieInput {
    pub name: String,
    pub value: String,
    pub url: Option<String>,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    pub expires: Option<f64>,
}

/// Viewport size.
#[derive(Debug, Clone)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: f64,
    pub mobile: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            device_scale_factor: 1.0,
            mobile: false,
        }
    }
}
