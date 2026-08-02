//! Browser + page session registry and CDP operations.

use crate::cdp::CdpConn;
use crate::config::{
    ConnectConfig, CookieInput, ImageFormat, LaunchConfig, NavOpts, PdfOpts, ScreenshotOpts,
    Viewport,
};
use crate::error::{BrowserError, BrowserResult};
use crate::runtime::block_on;
use crate::util::{
    js_string_literal, poll_until, require_selector, require_url, resolve_executable,
};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::time::sleep;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

struct BrowserState {
    cdp: Arc<CdpConn>,
    child: Option<StdMutex<Child>>,
    user_data_dir: Option<PathBuf>,
    alive: AtomicBool,
    pages: StdMutex<Vec<i64>>,
}

struct PageState {
    /// Shared browser connection with page sessionId set on each call via clone pattern.
    browser_cdp: Arc<CdpConn>,
    session_id: String,
    target_id: String,
    browser: i64,
    alive: AtomicBool,
}

static NEXT_ID: AtomicI64 = AtomicI64::new(1);

fn alloc_id() -> i64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

fn browsers() -> &'static StdMutex<HashMap<i64, Arc<BrowserState>>> {
    static M: std::sync::OnceLock<StdMutex<HashMap<i64, Arc<BrowserState>>>> =
        std::sync::OnceLock::new();
    M.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn pages_map() -> &'static StdMutex<HashMap<i64, Arc<PageState>>> {
    static M: std::sync::OnceLock<StdMutex<HashMap<i64, Arc<PageState>>>> =
        std::sync::OnceLock::new();
    M.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn get_browser(id: i64) -> BrowserResult<Arc<BrowserState>> {
    let map = browsers()
        .lock()
        .map_err(|_| BrowserError::msg("browser lock poisoned"))?;
    let b = map
        .get(&id)
        .cloned()
        .ok_or(BrowserError::InvalidHandle(id))?;
    if !b.alive.load(Ordering::Relaxed) {
        return Err(BrowserError::InvalidHandle(id));
    }
    Ok(b)
}

fn get_page(id: i64) -> BrowserResult<Arc<PageState>> {
    let map = pages_map()
        .lock()
        .map_err(|_| BrowserError::msg("page lock poisoned"))?;
    let p = map
        .get(&id)
        .cloned()
        .ok_or(BrowserError::InvalidHandle(id))?;
    if !p.alive.load(Ordering::Relaxed) {
        return Err(BrowserError::InvalidHandle(id));
    }
    Ok(p)
}

async fn page_call(page: &PageState, method: &str, params: JsonValue) -> BrowserResult<JsonValue> {
    // Temporarily set session on a wrapper by reconstructing call with sessionId.
    // CdpConn stores optional session_id; we use a per-call approach via raw message fields.
    call_with_session(&page.browser_cdp, Some(&page.session_id), method, params).await
}

async fn call_with_session(
    cdp: &CdpConn,
    session_id: Option<&str>,
    method: &str,
    params: JsonValue,
) -> BrowserResult<JsonValue> {
    // Clone connection identity: CdpConn isn't Clone; use the shared Arc and mutate session.
    // Instead bake session into a helper on CdpConn — call_session method.
    cdp.call_session(session_id, method, params).await
}

fn register_page(
    browser_id: i64,
    browser_cdp: Arc<CdpConn>,
    session_id: String,
    target_id: String,
) -> BrowserResult<i64> {
    let id = alloc_id();
    let state = Arc::new(PageState {
        browser_cdp,
        session_id,
        target_id,
        browser: browser_id,
        alive: AtomicBool::new(true),
    });
    pages_map()
        .lock()
        .map_err(|_| BrowserError::msg("page lock poisoned"))?
        .insert(id, state);
    if let Ok(b) = get_browser(browser_id) {
        if let Ok(mut list) = b.pages.lock() {
            list.push(id);
        }
    }
    Ok(id)
}

async fn wait_devtools_port(dir: &PathBuf, timeout: Duration) -> BrowserResult<(u16, String)> {
    let path = dir.join("DevToolsActivePort");
    let deadline = Instant::now() + timeout;
    loop {
        if path.is_file() {
            let text = fs::read_to_string(&path)
                .await
                .map_err(|e| BrowserError::Io(e.to_string()))?;
            let mut lines = text.lines();
            if let (Some(port_s), Some(path_s)) = (lines.next(), lines.next()) {
                if let Ok(port) = port_s.trim().parse::<u16>() {
                    let p = path_s.trim().to_string();
                    if port > 0 && !p.is_empty() {
                        return Ok((port, p));
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(BrowserError::Timeout(
                "browser did not publish DevToolsActivePort".into(),
            ));
        }
        sleep(Duration::from_millis(50)).await;
    }
}

async fn enable_page(cdp: &CdpConn, session_id: &str) -> BrowserResult<()> {
    let _ = call_with_session(cdp, Some(session_id), "Page.enable", json!({})).await?;
    let _ = call_with_session(cdp, Some(session_id), "Runtime.enable", json!({})).await?;
    let _ = call_with_session(cdp, Some(session_id), "DOM.enable", json!({})).await?;
    let _ = call_with_session(cdp, Some(session_id), "Network.enable", json!({})).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

pub fn launch(cfg: &LaunchConfig) -> BrowserResult<i64> {
    let exe = resolve_executable(cfg.executable.as_deref())?;
    let user_data = std::env::temp_dir().join(format!(
        "niao-nbrowser-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&user_data).map_err(|e| BrowserError::Io(e.to_string()))?;

    let mut args: Vec<String> = Vec::new();
    if cfg.headless {
        args.push("--headless=new".into());
    }
    args.push("--remote-debugging-port=0".into());
    args.push(format!("--user-data-dir={}", user_data.display()));
    args.push(format!("--window-size={},{}", cfg.width, cfg.height));
    args.push("--no-first-run".into());
    args.push("--no-default-browser-check".into());
    args.push("--disable-background-networking".into());
    args.push("--disable-sync".into());
    args.push("--disable-extensions".into());
    if cfg.no_sandbox {
        args.push("--no-sandbox".into());
        args.push("--disable-setuid-sandbox".into());
    }
    for a in &cfg.args {
        args.push(a.clone());
    }
    args.push("about:blank".into());

    let mut cmd = Command::new(&exe);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = cmd
        .spawn()
        .map_err(|e| BrowserError::Connect(format!("spawn {}: {e}", exe.display())))?;

    let timeout = Duration::from_millis(cfg.timeout_ms.max(1));
    block_on(async move {
        let (port, path) = wait_devtools_port(&user_data, timeout).await?;
        let ws = format!("ws://127.0.0.1:{port}{path}");
        let cdp = Arc::new(CdpConn::connect(&ws, timeout).await?);
        let id = alloc_id();
        let state = Arc::new(BrowserState {
            cdp,
            child: Some(StdMutex::new(child)),
            user_data_dir: Some(user_data),
            alive: AtomicBool::new(true),
            pages: StdMutex::new(Vec::new()),
        });
        browsers()
            .lock()
            .map_err(|_| BrowserError::msg("browser lock poisoned"))?
            .insert(id, state);
        Ok(id)
    })
}

pub fn connect(cfg: &ConnectConfig) -> BrowserResult<i64> {
    let endpoint = cfg.endpoint.trim().to_string();
    if endpoint.is_empty() {
        return Err(BrowserError::msg("connect() requires endpoint"));
    }
    let timeout = Duration::from_millis(cfg.timeout_ms.max(1));
    block_on(async move {
        let ws = if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
            endpoint
        } else {
            CdpConn::discover_ws(&endpoint, timeout).await?
        };
        let cdp = Arc::new(CdpConn::connect(&ws, timeout).await?);
        let id = alloc_id();
        let state = Arc::new(BrowserState {
            cdp,
            child: None,
            user_data_dir: None,
            alive: AtomicBool::new(true),
            pages: StdMutex::new(Vec::new()),
        });
        browsers()
            .lock()
            .map_err(|_| BrowserError::msg("browser lock poisoned"))?
            .insert(id, state);
        Ok(id)
    })
}

pub fn close(browser_id: i64) -> BrowserResult<()> {
    let state = {
        let mut map = browsers()
            .lock()
            .map_err(|_| BrowserError::msg("browser lock poisoned"))?;
        map.remove(&browser_id)
            .ok_or(BrowserError::InvalidHandle(browser_id))?
    };
    state.alive.store(false, Ordering::Relaxed);
    let page_ids: Vec<i64> = state
        .pages
        .lock()
        .map_err(|_| BrowserError::msg("page lock poisoned"))?
        .clone();
    for pid in page_ids {
        let _ = close_page(pid);
    }
    let _ = block_on(state.cdp.call("Browser.close", json!({})));
    if let Some(child_m) = &state.child {
        if let Ok(mut child) = child_m.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    if let Some(dir) = &state.user_data_dir {
        let _ = std::fs::remove_dir_all(dir);
    }
    Ok(())
}

pub fn is_connected(browser_id: i64) -> bool {
    get_browser(browser_id).is_ok()
}

pub fn version(browser_id: i64) -> BrowserResult<String> {
    let state = get_browser(browser_id)?;
    block_on(async move {
        let v = state.cdp.call("Browser.getVersion", json!({})).await?;
        let product = v
            .get("product")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown");
        let protocol = v
            .get("protocolVersion")
            .and_then(|x| x.as_str())
            .unwrap_or("?");
        let revision = v.get("revision").and_then(|x| x.as_str()).unwrap_or("?");
        Ok(format!("{product} {revision} ({protocol})"))
    })
}

pub fn new_page(browser_id: i64, url: Option<&str>) -> BrowserResult<i64> {
    let state = get_browser(browser_id)?;
    let target_url = match url {
        Some(u) => require_url(u)?.to_string(),
        None => "about:blank".to_string(),
    };
    block_on(async move {
        let created = state
            .cdp
            .call("Target.createTarget", json!({ "url": target_url }))
            .await?;
        let target_id = created
            .get("targetId")
            .and_then(|x| x.as_str())
            .ok_or_else(|| BrowserError::Protocol("createTarget missing targetId".into()))?
            .to_string();
        let attached = state
            .cdp
            .call(
                "Target.attachToTarget",
                json!({ "targetId": target_id, "flatten": true }),
            )
            .await?;
        let session_id = attached
            .get("sessionId")
            .and_then(|x| x.as_str())
            .ok_or_else(|| BrowserError::Protocol("attachToTarget missing sessionId".into()))?
            .to_string();
        enable_page(&state.cdp, &session_id).await?;
        register_page(browser_id, state.cdp.clone(), session_id, target_id)
    })
}

pub fn pages(browser_id: i64) -> BrowserResult<Vec<i64>> {
    let state = get_browser(browser_id)?;
    let list = state
        .pages
        .lock()
        .map_err(|_| BrowserError::msg("page lock poisoned"))?
        .clone();
    Ok(list
        .into_iter()
        .filter(|id| get_page(*id).is_ok())
        .collect())
}

pub fn close_page(page_id: i64) -> BrowserResult<()> {
    let state = {
        let mut map = pages_map()
            .lock()
            .map_err(|_| BrowserError::msg("page lock poisoned"))?;
        map.remove(&page_id)
            .ok_or(BrowserError::InvalidHandle(page_id))?
    };
    state.alive.store(false, Ordering::Relaxed);
    if let Ok(b) = get_browser(state.browser) {
        if let Ok(mut list) = b.pages.lock() {
            list.retain(|id| *id != page_id);
        }
    }
    let _ = block_on(
        state
            .browser_cdp
            .call("Target.closeTarget", json!({ "targetId": state.target_id })),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Navigation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GotoResult {
    pub url: String,
    pub title: String,
    pub ok: bool,
}

pub fn goto(page_id: i64, url: &str, opts: &NavOpts) -> BrowserResult<GotoResult> {
    let url = require_url(url)?.to_string();
    let state = get_page(page_id)?;
    let timeout = Duration::from_millis(opts.timeout_ms.max(1));
    block_on(async move {
        page_call(&state, "Page.navigate", json!({ "url": url })).await?;
        // Poll document readyState.
        let deadline = Instant::now() + timeout;
        loop {
            let rs = eval_raw(&state, "document.readyState").await?;
            if rs.as_str() == Some("complete") || rs.as_str() == Some("interactive") {
                break;
            }
            if Instant::now() >= deadline {
                return Err(BrowserError::Timeout("goto timed out".into()));
            }
            sleep(Duration::from_millis(50)).await;
        }
        let final_url = eval_raw(&state, "location.href")
            .await?
            .as_str()
            .unwrap_or("")
            .to_string();
        let title = eval_raw(&state, "document.title")
            .await?
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(GotoResult {
            url: final_url,
            title,
            ok: true,
        })
    })
}

pub fn reload(page_id: i64) -> BrowserResult<()> {
    let state = get_page(page_id)?;
    block_on(async move {
        page_call(&state, "Page.reload", json!({})).await?;
        sleep(Duration::from_millis(100)).await;
        Ok(())
    })
}

pub fn url(page_id: i64) -> BrowserResult<String> {
    let state = get_page(page_id)?;
    block_on(async move {
        Ok(eval_raw(&state, "location.href")
            .await?
            .as_str()
            .unwrap_or("")
            .to_string())
    })
}

pub fn title(page_id: i64) -> BrowserResult<String> {
    let state = get_page(page_id)?;
    block_on(async move {
        Ok(eval_raw(&state, "document.title")
            .await?
            .as_str()
            .unwrap_or("")
            .to_string())
    })
}

pub fn content(page_id: i64) -> BrowserResult<String> {
    let state = get_page(page_id)?;
    block_on(async move {
        Ok(eval_raw(&state, "document.documentElement.outerHTML")
            .await?
            .as_str()
            .unwrap_or("")
            .to_string())
    })
}

async fn eval_raw(page: &PageState, expression: &str) -> BrowserResult<JsonValue> {
    let result = page_call(
        page,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
        }),
    )
    .await?;
    if let Some(exc) = result.get("exceptionDetails") {
        return Err(BrowserError::Protocol(
            exc.get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("JS exception")
                .to_string(),
        ));
    }
    Ok(result
        .pointer("/result/value")
        .cloned()
        .unwrap_or(JsonValue::Null))
}

pub fn eval(page_id: i64, expression: &str) -> BrowserResult<JsonValue> {
    if expression.trim().is_empty() {
        return Err(BrowserError::msg("eval() expression must not be empty"));
    }
    let state = get_page(page_id)?;
    let expr = expression.to_string();
    block_on(async move { eval_raw(&state, &expr).await })
}

// ---------------------------------------------------------------------------
// Interaction
// ---------------------------------------------------------------------------

async fn query_one(page: &PageState, selector: &str) -> BrowserResult<()> {
    let js = format!(
        "(function(){{ var el=document.querySelector({}); if(!el) throw new Error('not found'); return true; }})()",
        js_string_literal(selector)
    );
    eval_raw(page, &js).await?;
    Ok(())
}

pub fn click(page_id: i64, selector: &str) -> BrowserResult<()> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    block_on(async move {
        query_one(&state, &sel)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel.clone()))?;
        let js = format!(
            "(function(){{ var el=document.querySelector({}); el.scrollIntoView({{block:'center'}}); el.click(); return true; }})()",
            js_string_literal(&sel)
        );
        eval_raw(&state, &js).await?;
        Ok(())
    })
}

pub fn fill(page_id: i64, selector: &str, text: &str) -> BrowserResult<()> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    let text = text.to_string();
    block_on(async move {
        query_one(&state, &sel)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel.clone()))?;
        let js = format!(
            "(function(){{ var el=document.querySelector({}); el.focus(); if('value' in el){{ el.value={}; }} else {{ el.textContent={}; }} el.dispatchEvent(new Event('input',{{bubbles:true}})); el.dispatchEvent(new Event('change',{{bubbles:true}})); return true; }})()",
            js_string_literal(&sel),
            js_string_literal(&text),
            js_string_literal(&text)
        );
        eval_raw(&state, &js).await?;
        Ok(())
    })
}

pub fn type_text(page_id: i64, selector: &str, text: &str) -> BrowserResult<()> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    let text = text.to_string();
    block_on(async move {
        query_one(&state, &sel)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel.clone()))?;
        let js = format!(
            "(function(){{ document.querySelector({}).focus(); return true; }})()",
            js_string_literal(&sel)
        );
        eval_raw(&state, &js).await?;
        page_call(&state, "Input.insertText", json!({ "text": text })).await?;
        Ok(())
    })
}

pub fn press(page_id: i64, key: &str) -> BrowserResult<()> {
    if key.trim().is_empty() {
        return Err(BrowserError::msg("press() key must not be empty"));
    }
    let state = get_page(page_id)?;
    let key = key.to_string();
    block_on(async move {
        for typ in ["keyDown", "keyUp"] {
            page_call(
                &state,
                "Input.dispatchKeyEvent",
                json!({
                    "type": typ,
                    "key": key,
                    "text": if key.len() == 1 { JsonValue::String(key.clone()) } else { JsonValue::Null },
                }),
            )
            .await?;
        }
        Ok(())
    })
}

pub fn hover(page_id: i64, selector: &str) -> BrowserResult<()> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    block_on(async move {
        let js = format!(
            "(function(){{ var el=document.querySelector({}); if(!el) throw new Error('not found'); var r=el.getBoundingClientRect(); return {{x:r.x+r.width/2,y:r.y+r.height/2}}; }})()",
            js_string_literal(&sel)
        );
        let pt = eval_raw(&state, &js)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel))?;
        let x = pt.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let y = pt.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
        page_call(
            &state,
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseMoved", "x": x, "y": y }),
        )
        .await?;
        Ok(())
    })
}

pub fn focus(page_id: i64, selector: &str) -> BrowserResult<()> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    block_on(async move {
        let js = format!(
            "(function(){{ var el=document.querySelector({}); if(!el) throw new Error('not found'); el.focus(); return true; }})()",
            js_string_literal(&sel)
        );
        eval_raw(&state, &js)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel))?;
        Ok(())
    })
}

pub fn select_option(page_id: i64, selector: &str, value: &str) -> BrowserResult<()> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    let value = value.to_string();
    block_on(async move {
        let js = format!(
            "(function(){{ var el=document.querySelector({}); if(!el) throw new Error('not found'); el.value={}; el.dispatchEvent(new Event('input',{{bubbles:true}})); el.dispatchEvent(new Event('change',{{bubbles:true}})); return true; }})()",
            js_string_literal(&sel),
            js_string_literal(&value)
        );
        eval_raw(&state, &js)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel))?;
        Ok(())
    })
}

pub fn check(page_id: i64, selector: &str) -> BrowserResult<()> {
    set_checked(page_id, selector, true)
}

pub fn uncheck(page_id: i64, selector: &str) -> BrowserResult<()> {
    set_checked(page_id, selector, false)
}

fn set_checked(page_id: i64, selector: &str, want: bool) -> BrowserResult<()> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    block_on(async move {
        let js = format!(
            "(function(){{ var el=document.querySelector({}); if(!el) throw new Error('not found'); if(!!el.checked !== {}); el.click(); return !!el.checked; }})()",
            js_string_literal(&sel),
            want
        );
        eval_raw(&state, &js)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel))?;
        Ok(())
    })
}

pub fn wait_for(page_id: i64, selector: &str, timeout_ms: u64) -> BrowserResult<()> {
    let sel = require_selector(selector)?.to_string();
    let _ = get_page(page_id)?;
    let timeout = Duration::from_millis(timeout_ms.max(1));
    poll_until(timeout, || {
        let state = get_page(page_id)?;
        let found = block_on(async {
            match query_one(&state, &sel).await {
                Ok(()) => Ok(true),
                Err(_) => Ok::<bool, BrowserError>(false),
            }
        })?;
        Ok(if found { Some(()) } else { None })
    })
}

pub fn text_content(page_id: i64, selector: &str) -> BrowserResult<String> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    block_on(async move {
        let js = format!(
            "(function(){{ var el=document.querySelector({}); if(!el) throw new Error('not found'); return el.innerText || ''; }})()",
            js_string_literal(&sel)
        );
        let v = eval_raw(&state, &js)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel))?;
        Ok(v.as_str().unwrap_or("").to_string())
    })
}

pub fn attr(page_id: i64, selector: &str, name: &str) -> BrowserResult<Option<String>> {
    let sel = require_selector(selector)?.to_string();
    if name.trim().is_empty() {
        return Err(BrowserError::msg("attr() name must not be empty"));
    }
    let state = get_page(page_id)?;
    let name = name.to_string();
    block_on(async move {
        let js = format!(
            "(function(){{ var el=document.querySelector({}); if(!el) throw new Error('not found'); return el.getAttribute({}); }})()",
            js_string_literal(&sel),
            js_string_literal(&name)
        );
        let v = eval_raw(&state, &js)
            .await
            .map_err(|_| BrowserError::SelectorNotFound(sel))?;
        Ok(v.as_str().map(|s| s.to_string()))
    })
}

pub fn exists(page_id: i64, selector: &str) -> BrowserResult<bool> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    block_on(async move { Ok(query_one(&state, &sel).await.is_ok()) })
}

pub fn count(page_id: i64, selector: &str) -> BrowserResult<i64> {
    let sel = require_selector(selector)?.to_string();
    let state = get_page(page_id)?;
    block_on(async move {
        let js = format!(
            "document.querySelectorAll({}).length",
            js_string_literal(&sel)
        );
        let v = eval_raw(&state, &js).await?;
        Ok(v.as_i64()
            .or_else(|| v.as_f64().map(|f| f as i64))
            .unwrap_or(0))
    })
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

pub fn screenshot(page_id: i64, opts: &ScreenshotOpts) -> BrowserResult<Vec<u8>> {
    let state = get_page(page_id)?;
    let format = match opts.format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Webp => "webp",
    };
    let full = opts.full_page;
    let quality = opts.quality;
    block_on(async move {
        let mut params = json!({
            "format": format,
            "captureBeyondViewport": full,
        });
        if let Some(q) = quality {
            params
                .as_object_mut()
                .unwrap()
                .insert("quality".into(), json!(q));
        }
        if full {
            // Expand viewport to content height for full-page capture.
            let metrics = page_call(&state, "Page.getLayoutMetrics", json!({}))
                .await
                .ok();
            if let Some(m) = metrics {
                if let Some(css) = m.get("cssContentSize").or_else(|| m.get("contentSize")) {
                    let w = css.get("width").and_then(|v| v.as_f64()).unwrap_or(1280.0);
                    let h = css.get("height").and_then(|v| v.as_f64()).unwrap_or(720.0);
                    let _ = page_call(
                        &state,
                        "Emulation.setDeviceMetricsOverride",
                        json!({
                            "width": w.ceil() as u64,
                            "height": h.ceil() as u64,
                            "deviceScaleFactor": 1,
                            "mobile": false,
                        }),
                    )
                    .await;
                }
            }
        }
        let result = page_call(&state, "Page.captureScreenshot", params).await?;
        let b64 = result
            .get("data")
            .and_then(|d| d.as_str())
            .ok_or_else(|| BrowserError::Protocol("screenshot missing data".into()))?;
        decode_b64(b64)
    })
}

pub fn pdf(page_id: i64, opts: &PdfOpts) -> BrowserResult<Vec<u8>> {
    let state = get_page(page_id)?;
    let landscape = opts.landscape;
    let print_background = opts.print_background;
    let scale = opts.scale;
    let paper_width = opts.paper_width;
    let paper_height = opts.paper_height;
    block_on(async move {
        let mut params = json!({
            "landscape": landscape,
            "printBackground": print_background,
            "scale": scale,
        });
        if let Some(w) = paper_width {
            params
                .as_object_mut()
                .unwrap()
                .insert("paperWidth".into(), json!(w));
        }
        if let Some(h) = paper_height {
            params
                .as_object_mut()
                .unwrap()
                .insert("paperHeight".into(), json!(h));
        }
        let result = page_call(&state, "Page.printToPDF", params).await?;
        let b64 = result
            .get("data")
            .and_then(|d| d.as_str())
            .ok_or_else(|| BrowserError::Protocol("pdf missing data".into()))?;
        decode_b64(b64)
    })
}

fn decode_b64(s: &str) -> BrowserResult<Vec<u8>> {
    fn decode(input: &str) -> Result<Vec<u8>, &'static str> {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut table = [0u8; 256];
        for (i, &c) in T.iter().enumerate() {
            table[c as usize] = i as u8;
        }
        let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        if clean.len() % 4 != 0 {
            return Err("bad base64 length");
        }
        let mut out = Vec::with_capacity(clean.len() / 4 * 3);
        for chunk in clean.chunks_exact(4) {
            let a = table[chunk[0] as usize] as u32;
            let b = table[chunk[1] as usize] as u32;
            let pad2 = chunk[2] == b'=';
            let pad3 = chunk[3] == b'=';
            let c = if pad2 {
                0
            } else {
                table[chunk[2] as usize] as u32
            };
            let d = if pad3 {
                0
            } else {
                table[chunk[3] as usize] as u32
            };
            let n = (a << 18) | (b << 12) | (c << 6) | d;
            out.push(((n >> 16) & 0xff) as u8);
            if !pad2 {
                out.push(((n >> 8) & 0xff) as u8);
            }
            if !pad3 {
                out.push((n & 0xff) as u8);
            }
        }
        Ok(out)
    }
    decode(s).map_err(|e| BrowserError::Protocol(e.into()))
}

// ---------------------------------------------------------------------------
// Page config
// ---------------------------------------------------------------------------

pub fn set_viewport(page_id: i64, vp: &Viewport) -> BrowserResult<()> {
    let state = get_page(page_id)?;
    let width = vp.width;
    let height = vp.height;
    let dsf = vp.device_scale_factor;
    let mobile = vp.mobile;
    block_on(async move {
        page_call(
            &state,
            "Emulation.setDeviceMetricsOverride",
            json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": dsf,
                "mobile": mobile,
            }),
        )
        .await?;
        Ok(())
    })
}

pub fn set_extra_headers(page_id: i64, headers: HashMap<String, String>) -> BrowserResult<()> {
    let state = get_page(page_id)?;
    block_on(async move {
        let mut obj = serde_json::Map::new();
        for (k, v) in headers {
            obj.insert(k, JsonValue::String(v));
        }
        page_call(
            &state,
            "Network.setExtraHTTPHeaders",
            json!({ "headers": JsonValue::Object(obj) }),
        )
        .await?;
        Ok(())
    })
}

#[derive(Debug, Clone)]
pub struct CookieInfo {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub expires: f64,
}

pub fn cookies(page_id: i64) -> BrowserResult<Vec<CookieInfo>> {
    let state = get_page(page_id)?;
    block_on(async move {
        let result = page_call(&state, "Network.getCookies", json!({})).await?;
        let list = result
            .get("cookies")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(list
            .into_iter()
            .map(|c| CookieInfo {
                name: c.get("name").and_then(|v| v.as_str()).unwrap_or("").into(),
                value: c.get("value").and_then(|v| v.as_str()).unwrap_or("").into(),
                domain: c
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .into(),
                path: c.get("path").and_then(|v| v.as_str()).unwrap_or("").into(),
                secure: c.get("secure").and_then(|v| v.as_bool()).unwrap_or(false),
                http_only: c.get("httpOnly").and_then(|v| v.as_bool()).unwrap_or(false),
                expires: c.get("expires").and_then(|v| v.as_f64()).unwrap_or(-1.0),
            })
            .collect())
    })
}

pub fn set_cookie(page_id: i64, cookie: &CookieInput) -> BrowserResult<()> {
    if cookie.name.is_empty() {
        return Err(BrowserError::msg("set_cookie() name must not be empty"));
    }
    let state = get_page(page_id)?;
    let cookie = cookie.clone();
    block_on(async move {
        let mut obj = json!({
            "name": cookie.name,
            "value": cookie.value,
            "secure": cookie.secure,
            "httpOnly": cookie.http_only,
        });
        if let Some(url) = cookie.url {
            obj.as_object_mut()
                .unwrap()
                .insert("url".into(), json!(url));
        }
        if let Some(domain) = cookie.domain {
            obj.as_object_mut()
                .unwrap()
                .insert("domain".into(), json!(domain));
        }
        if let Some(path) = cookie.path {
            obj.as_object_mut()
                .unwrap()
                .insert("path".into(), json!(path));
        }
        if let Some(exp) = cookie.expires {
            obj.as_object_mut()
                .unwrap()
                .insert("expires".into(), json!(exp));
        }
        page_call(&state, "Network.setCookie", obj).await?;
        Ok(())
    })
}

pub fn clear_cookies(page_id: i64) -> BrowserResult<()> {
    let state = get_page(page_id)?;
    block_on(async move {
        page_call(&state, "Network.clearBrowserCookies", json!({})).await?;
        Ok(())
    })
}

pub fn page_alive(page_id: i64) -> bool {
    get_page(page_id).is_ok()
}
