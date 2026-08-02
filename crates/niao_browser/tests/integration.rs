//! Integration tests for niao_browser.
//! Offline tests always run. Live Chrome/Edge tests: `cargo test -p niao_browser -- --ignored`.

use niao_browser::{
    check, click, close, content, count, eval, executable_path, exists, fill, goto, is_connected,
    launch, new_page, pdf, require_selector, screenshot, text_content, uncheck, ImageFormat,
    LaunchConfig, NavOpts, PdfOpts, ScreenshotOpts,
};

#[test]
fn selector_and_url_validation() {
    assert!(require_selector("").is_err());
    assert!(require_selector("  ").is_err());
    assert_eq!(require_selector("#a").unwrap(), "#a");
}

#[test]
fn invalid_handles_are_errors() {
    assert!(!is_connected(999_999));
    assert!(goto(999_999, "about:blank", &NavOpts::default()).is_err());
    assert!(click(999_999, "#x").is_err());
    assert!(screenshot(999_999, &ScreenshotOpts::default()).is_err());
    assert!(pdf(999_999, &PdfOpts::default()).is_err());
}

#[test]
fn launch_missing_executable() {
    let cfg = LaunchConfig {
        executable: Some(
            if cfg!(windows) {
                r"C:\no\such\nbrowser-chrome.exe"
            } else {
                "/no/such/nbrowser-chrome"
            }
            .into(),
        ),
        timeout_ms: 500,
        ..LaunchConfig::default()
    };
    let err = launch(&cfg).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("not found") || msg.contains("executable"),
        "unexpected: {msg}"
    );
}

#[test]
fn executable_path_detection() {
    // On this Windows CI host Edge/Chrome is present; elsewhere may be None.
    let _ = executable_path();
}

fn live_available() -> bool {
    executable_path().is_some()
}

#[test]
#[ignore = "requires Chrome/Edge"]
fn live_about_blank_eval() {
    if !live_available() {
        return;
    }
    let b = launch(&LaunchConfig {
        headless: true,
        no_sandbox: true,
        timeout_ms: 60_000,
        ..LaunchConfig::default()
    })
    .expect("launch");
    assert!(is_connected(b));
    let p = new_page(b, Some("about:blank")).expect("page");
    let v = eval(p, "1 + 2").expect("eval");
    assert_eq!(v.as_i64(), Some(3));
    close(b).expect("close");
    assert!(!is_connected(b));
}

#[test]
#[ignore = "requires Chrome/Edge"]
fn live_data_url_click_fill_screenshot_pdf() {
    if !live_available() {
        return;
    }
    let html = r#"data:text/html,<!doctype html><html><body>
        <h1 id="t">Hello</h1>
        <input id="q" value=""/>
        <button id="btn" onclick="document.getElementById('t').textContent='Clicked'">Go</button>
        <input id="c" type="checkbox"/>
        <ul><li>a</li><li>b</li><li>c</li></ul>
        </body></html>"#;
    let b = launch(&LaunchConfig {
        headless: true,
        no_sandbox: true,
        timeout_ms: 60_000,
        ..LaunchConfig::default()
    })
    .expect("launch");
    let p = new_page(b, None).expect("page");
    let r = goto(p, html, &NavOpts::default()).expect("goto");
    assert!(r.ok);
    assert_eq!(text_content(p, "#t").unwrap(), "Hello");
    assert!(exists(p, "#btn").unwrap());
    assert_eq!(count(p, "li").unwrap(), 3);
    fill(p, "#q", "niào 🐱").unwrap();
    let typed = eval(p, "document.querySelector('#q').value").unwrap();
    assert_eq!(typed.as_str(), Some("niào 🐱"));
    click(p, "#btn").unwrap();
    assert_eq!(text_content(p, "#t").unwrap(), "Clicked");
    check(p, "#c").unwrap();
    assert_eq!(
        eval(p, "document.querySelector('#c').checked")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    uncheck(p, "#c").unwrap();
    let png = screenshot(
        p,
        &ScreenshotOpts {
            full_page: true,
            format: ImageFormat::Png,
            quality: None,
        },
    )
    .expect("screenshot");
    assert!(png.len() > 100);
    assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
    let pdf_bytes = pdf(p, &PdfOpts::default()).expect("pdf");
    assert!(pdf_bytes.len() > 100);
    assert_eq!(&pdf_bytes[0..4], b"%PDF");
    let html_out = content(p).unwrap();
    assert!(html_out.contains("Clicked"));
    close(b).unwrap();
}
