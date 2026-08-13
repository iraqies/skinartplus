use crate::commands::files::http_client;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::State;
use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

fn curl_path() -> &'static str {
    if cfg!(windows) && std::path::Path::new("C:\\Windows\\System32\\curl.exe").exists() {
        "C:\\Windows\\System32\\curl.exe"
    } else {
        "curl"
    }
}

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// TLS fingerprints to emulate when talking to NameMC. NameMC is behind
/// Cloudflare, which blocks plain HTTP clients (the dreaded 403). `wreq`
/// impersonates real browser TLS/HTTP fingerprints, so we try several
/// profiles until one is not challenged.
const EMULATION_TARGETS: &[wreq_util::Profile] = &[
    wreq_util::Profile::Chrome149,
    wreq_util::Profile::Chrome140,
    wreq_util::Profile::Safari26,
    wreq_util::Profile::Edge148,
    wreq_util::Profile::Firefox149,
    wreq_util::Profile::Chrome120,
    wreq_util::Profile::Safari17_0,
];

/// True when the response is a Cloudflare interstitial rather than the profile.
///
/// NOTE: `challenge-platform` is deliberately NOT checked here — every
/// Cloudflare-hosted page loads a beacon script from `challenge-platform/...`,
/// so that string appears on perfectly good pages too.
fn is_challenge(html: &str) -> bool {
    html.contains("Just a moment")
        || html.contains("cf-chl-")
        || html.contains("__cf_chl")
        || html.contains("Verify you are human")
        || html.contains("Checking your browser")
}

/// Fetch the NameMC profile page for an IGN.
///
/// NameMC is behind Cloudflare and sometimes blocks plain HTTP clients. The
/// Windows system curl.exe (built on SChannel) usually passes Cloudflare's
/// checks, so we try it first (with its console window suppressed). When curl
/// is unavailable (e.g. Linux builds / the Flatpak sandbox) or gets challenged,
/// we fall back to reqwest with full browser headers. As a last resort we load
/// the profile in a hidden WebView2 window, which runs a real browser engine and
/// passes Cloudflare's JS challenges; the skin hashes are collected from the
/// rendered DOM and reassembled into an HTML fragment for `extract_hashes`.
/// Fetch the NameMC skin list page for an IGN and return an HTML fragment
/// carrying the skin hashes, so downstream callers can keep using
/// `extract_hashes` regardless of the data source.
///
/// Order of attempts (each one covers a different failure mode):
///   1. `wreq` with TLS fingerprint emulation — passes Cloudflare on NameMC.
///   2. Dripfy.io's plain JSON API (mirrors NameMC, no challenge).
///   3. Windows system curl (SChannel).
///   4. reqwest with full browser headers.
///   5. A hidden WebView2 window rendering the real page (last resort).
async fn fetch_profile_html(app: &AppHandle, ign: &str) -> Result<String, String> {
    // 1. wreq with browser TLS emulation (the reliable path).
    match wreq_namemc_skins(ign).await {
        Ok(html) => {
            crate::dbg_log!("fetch_profile_html: got NameMC skins page via wreq");
            return Ok(html);
        }
        Err(e) => crate::dbg_log!("fetch_profile_html: wreq failed ({e}), trying Dripfy"),
    }

    // 2. Dripfy.io mirrors NameMC's skin data behind a plain JSON API that
    //    Cloudflare does not challenge.
    match fetch_dripfy_hashes(ign).await {
        Ok(hashes) if !hashes.is_empty() => {
            crate::dbg_log!("fetch_profile_html: got {} hashes from Dripfy", hashes.len());
            return Ok(hashes_html(&hashes));
        }
        Ok(_) => crate::dbg_log!("fetch_profile_html: Dripfy returned no skins"),
        Err(e) => crate::dbg_log!("fetch_profile_html: Dripfy failed ({e})"),
    }

    let url = format!("https://namemc.com/profile/{}", urlencoding::encode(ign));

    // 3. Try curl with browser-like headers.
    match curl_fetch(&url).await {
        Ok(html) if !html.trim().is_empty() => {
            if !is_challenge(&html) {
                return Ok(html);
            }
            crate::dbg_log!("fetch_profile_html: curl got a Cloudflare challenge, trying reqwest");
        }
        Err(e) if e.contains("Failed to launch curl") => {
            crate::dbg_log!("fetch_profile_html: curl unavailable ({e}), using reqwest");
        }
        Err(e) => {
            crate::dbg_log!("fetch_profile_html: curl error ({e}), trying reqwest");
        }
        _ => {}
    }

    // 4. Fall back to reqwest with full browser headers and one retry.
    match reqwest_fetch(&url).await {
        Ok(html) if !html.trim().is_empty() => {
            if !is_challenge(&html) {
                return Ok(html);
            }
            crate::dbg_log!("fetch_profile_html: reqwest challenged, trying WebView2");
        }
        Ok(_) => crate::dbg_log!("fetch_profile_html: empty response, trying WebView2"),
        Err(e) => crate::dbg_log!("fetch_profile_html: reqwest error ({e}), trying WebView2"),
    }

    // 5. Last resort: render the profile in a hidden WebView2 window.
    let hashes = scrape_hashes_webview(app, ign).await?;
    if hashes.is_empty() {
        return Err("No skins found on NameMC profile".into());
    }
    Ok(hashes_html(&hashes))
}

/// Build a minimal HTML fragment carrying the skin hashes, so downstream
/// callers can keep using `extract_hashes` regardless of the data source.
fn hashes_html(hashes: &[String]) -> String {
    hashes
        .iter()
        .map(|h| format!("\n<div data-id=\"{}\"></div>", h))
        .collect::<String>()
}

/// Pull skin hashes from Dripfy.io's public search API, which mirrors
/// NameMC's skin history. The active skin hash comes first.
async fn fetch_dripfy_hashes(ign: &str) -> Result<Vec<String>, String> {
    let client = http_client();
    let url = format!("https://dripfy.io/api/search?q={}", urlencoding::encode(ign));
    let resp = client
        .get(&url)
        .header("User-Agent", BROWSER_UA)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("Dripfy request failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Dripfy API returned HTTP {}", resp.status().as_u16()));
    }
    let json: Value = resp.json().await.map_err(|e| format!("Dripfy bad response: {e}"))?;
    if json.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err("Dripfy: player not found".into());
    }

    let mut hashes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |h: String| {
        if seen.insert(h.clone()) {
            hashes.push(h);
        }
    };

    if let Some(raw) = json.get("skin").and_then(|s| s.get("raw")).and_then(|v| v.as_str()) {
        if let Some(h) = hash_from_skins_url(raw) {
            push(h);
        }
    }
    if let Some(arr) = json.get("skins").and_then(|v| v.as_array()) {
        for s in arr {
            if let Some(id) = s.get("skin_id").and_then(|v| v.as_str()) {
                push(id.to_string());
            }
            for key in ["raw", "body", "face"] {
                if let Some(u) = s.get(key).and_then(|v| v.as_str()) {
                    if let Some(h) = hash_from_skins_url(u) {
                        push(h);
                    }
                }
            }
        }
    }
    Ok(hashes)
}

/// Extract the 16-hex skin hash from a Dripfy/NameMC texture URL
/// (`/i/{hash}.png` in the path or `id={hash}` in the query).
fn hash_from_skins_url(url: &str) -> Option<String> {
    let re = Regex::new(r"(?i)(?:[?&]id=|/i/)([a-f0-9]{16})").unwrap();
    re.captures(url).map(|c| c[1].to_ascii_lowercase())
}

/// Build a wreq client whose TLS/HTTP fingerprints pass Cloudflare, probing
/// `url` with each emulation profile until one returns a non-challenge 200.
/// Mirrors the working reference scraper.
async fn build_wreq_client(url: &str) -> Result<wreq::Client, String> {
    let mut last_err = None;
    for target in EMULATION_TARGETS {
        let client = match wreq::Client::builder()
            .emulation(*target)
            .redirect(wreq::redirect::Policy::limited(10))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                last_err = Some(format!("client build failed: {e}"));
                continue;
            }
        };
        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if status.as_u16() == 200 && !is_challenge(&body) {
                    return Ok(client);
                }
                last_err = Some(format!("blocked (HTTP {status})"));
            }
            Err(e) => last_err = Some(e.to_string()),
        }
    }
    Err(format!(
        "NameMC blocked all emulation profiles; last error: {}",
        last_err.unwrap_or_else(|| "unknown".into())
    ))
}

/// Follow the `/profile/{name}` redirect to the canonical profile name
/// (e.g. `Iraqies` -> `Iraqies.1`). Non-canonical names 404 on the skins page,
/// so this must run before scraping skins.
async fn resolve_canonical_name(client: &wreq::Client, name: &str) -> Result<String, String> {
    let url = format!("https://namemc.com/profile/{}", urlencoding::encode(name));
    let resp = client
        .get(&url)
        .redirect(wreq::redirect::Policy::none())
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Err(format!("profile not found: {name}"));
    }
    if (301..=399).contains(&status) {
        if let Some(loc) = resp.headers().get("location") {
            let loc = loc.to_str().unwrap_or("").to_string();
            if let Some(idx) = loc.find("/profile/") {
                let canonical = loc[idx + "/profile/".len()..].trim_end_matches('/').to_string();
                if !canonical.is_empty() {
                    return Ok(canonical);
                }
            }
        }
    }
    Ok(name.to_string())
}

/// Scrape the NameMC skins page for `ign` using a wreq client with browser TLS
/// emulation: build a passing client, resolve the canonical name, then fetch
/// `/minecraft-skins/profile/{canonical}` and extract the `/skin/{hash}` links.
async fn wreq_namemc_skins(ign: &str) -> Result<String, String> {
    let probe_url = format!("https://namemc.com/profile/{}", urlencoding::encode(ign));
    let client = build_wreq_client(&probe_url).await?;
    let canonical = resolve_canonical_name(&client, ign).await?;
    let skins_url = format!(
        "https://namemc.com/minecraft-skins/profile/{}",
        urlencoding::encode(&canonical)
    );
    let resp = client
        .get(&skins_url)
        .send()
        .await
        .map_err(|e| format!("skins fetch failed: {e}"))?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    if is_challenge(&body) {
        return Err("skins page is a Cloudflare challenge".into());
    }
    Ok(body)
}

/// Download rendered 2D face previews for every skin on the profile, exactly
/// Extract the native 8x8 face from a raw skin PNG, with the outer (hat)
/// layer composited on top for 64x64 skins (Minecraft layers base + overlay).
/// Old 64x32 skins have no separate outer layer, so the base face is used
/// as-is. Rendered at native resolution — the UI upscales it, so it always
/// matches the real skin pixels instead of a server-side scaled render.
fn skin_face_png(skin_png: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(skin_png)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let (w, h) = img.dimensions();
    if w < 48 || h < 16 {
        return Err(format!("skin image too small ({w}x{h})"));
    }
    let mut face = image::imageops::crop_imm(&img, 8, 8, 8, 8).to_image();
    if h == 64 {
        let overlay = image::imageops::crop_imm(&img, 40, 8, 8, 8).to_image();
        for y in 0..8 {
            for x in 0..8 {
                let p = *overlay.get_pixel(x, y);
                if p[3] > 0 {
                    face.put_pixel(x, y, p);
                }
            }
        }
    }
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(face)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Download the raw skin PNGs for `hashes` with bounded parallelism.
/// Results are returned in `hashes` order, regardless of completion order.
async fn download_skins_parallel(hashes: &[String]) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(hashes.len());
    for chunk in hashes.chunks(6) {
        let mut set = tokio::task::JoinSet::new();
        for (i, h) in chunk.iter().enumerate() {
            let url = format!("https://s.namemc.com/i/{}.png", h);
            set.spawn(async move { (i, crate::commands::files::download_bytes(&url).await.ok()) });
        }
        let mut ordered: Vec<Option<Vec<u8>>> = vec![None; chunk.len()];
        while let Some(res) = set.join_next().await {
            if let Ok((i, Some(bytes))) = res {
                ordered[i] = Some(bytes);
            }
        }
        out.extend(ordered.into_iter().flatten());
    }
    out
}

const NAMEMC_SCRAPE_EVENT: &str = "namemc-scrape-result";

/// Render `https://namemc.com/profile/{ign}` in a hidden WebView2 window and
/// collect the skin hashes from the rendered DOM. The real browser engine
/// passes Cloudflare's checks that block plain HTTP clients.
async fn scrape_hashes_webview(app: &AppHandle, ign: &str) -> Result<Vec<String>, String> {
    let url = format!("https://namemc.com/profile/{}", urlencoding::encode(ign));
    let parsed_url = tauri::Url::parse(&url).map_err(|e| format!("Invalid URL: {e}"))?;

    let (tx, rx) = tokio::sync::oneshot::channel::<Vec<String>>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let tx_listener = tx.clone();

    let listener_id = app.listen(NAMEMC_SCRAPE_EVENT, move |event| {
        let payload = event.payload();
        if let Ok(v) = serde_json::from_str::<Value>(payload) {
            let hashes: Vec<String> = v
                .get("hashes")
                .and_then(|h| h.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if !hashes.is_empty() {
                if let Some(sender) = tx_listener.lock().unwrap().take() {
                    let _ = sender.send(hashes);
                }
            }
        }
    });

    let script = r#"
        (function () {
            if (!window.__TAURI__ || !window.__TAURI__.event) return;
            var sent = false;
            var seen = [];
            function collect() {
                if (sent) return;
                var els = document.querySelectorAll('[data-id]');
                var hashes = [];
                for (var i = 0; i < els.length; i++) {
                    var id = (els[i].getAttribute('data-id') || '').trim();
                    if (/^[a-f0-9]{16}$/.test(id) && seen.indexOf(id) === -1) {
                        seen.push(id);
                        hashes.push(id);
                    }
                }
                if (hashes.length > 0) {
                    sent = true;
                    window.__TAURI__.event.emit('namemc-scrape-result', { hashes: hashes });
                }
            }
            setInterval(collect, 400);
        })();
    "#;

    if let Some(existing) = app.get_webview_window("namemc-scrape") {
        let _ = existing.close();
    }
    let window = WebviewWindowBuilder::new(app, "namemc-scrape", WebviewUrl::External(parsed_url))
        .title("")
        .inner_size(1100.0, 820.0)
        .visible(false)
        .skip_taskbar(true)
        .focusable(false)
        .initialization_script(script)
        .build()
        .map_err(|e| format!("Failed to open NameMC in the background browser: {e}"))?;

    let result = match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
        Ok(Ok(hashes)) => Ok(hashes),
        Ok(Err(_)) => Err("NameMC scraper was interrupted".into()),
        Err(_) => Err(
            "Timed out waiting for NameMC in the background browser. The page may be stuck on a Cloudflare check — try again in a few seconds."
                .into(),
        ),
    };

    let _ = window.close();
    app.unlisten(listener_id);
    result
}

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Spawn curl with its console window suppressed so no cmd flashes on screen.
async fn curl_fetch(url: &str) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new(curl_path());
    cmd.arg("-sSL")
        .arg("--compressed")
        .arg("--max-time")
        .arg("25")
        .arg("--retry")
        .arg("3")
        .arg("--retry-all-errors")
        .arg("--retry-delay")
        .arg("2")
        .arg("-A")
        .arg(BROWSER_UA)
        .arg("-H")
        .arg("Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .arg("-H")
        .arg("Accept-Language: en-US,en;q=0.9")
        .arg(&url);
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to launch curl: {}", e))?;
    if !output.status.success() {
        return Err(format!(
            "NameMC fetch failed (curl exit {}): {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn reqwest_fetch(url: &str) -> Result<String, String> {
    let client = http_client();
    let mut last_err: Option<String> = None;
    for attempt in 1..=3 {
        let resp = client
            .get(url)
            .header("User-Agent", BROWSER_UA)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
            )
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("sec-ch-ua", "\"Not_A Brand\";v=\"8\", \"Chromium\";v=\"120\", \"Google Chrome\";v=\"120\"")
            .header("sec-ch-ua-mobile", "?0")
            .header("sec-ch-ua-platform", "\"Windows\"")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Upgrade-Insecure-Requests", "1")
            .header("Referer", "https://namemc.com/")
            .send()
            .await;
        match resp {
            Ok(r) => {
                if r.status().is_success() {
                    return r.text().await.map_err(|e| e.to_string());
                }
                last_err = Some(format!("HTTP {}", r.status().as_u16()));
            }
            Err(e) => last_err = Some(e.to_string()),
        }
        let backoff = match attempt {
            1 => 1500,
            2 => 4000,
            _ => 0,
        };
        if backoff > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(backoff)).await;
        }
    }
    let msg = last_err.unwrap_or_else(|| "unknown error".into());
    if msg == "HTTP 403" {
        Err("NameMC is temporarily blocking this request (HTTP 403). Too many rapid requests can trigger a short block — wait a minute and try again.".into())
    } else {
        Err(format!("NameMC fetch failed: {}", msg))
    }
}

fn extract_hashes(html: &str) -> Vec<String> {
    let mut hashes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |h: String| {
        if seen.insert(h.clone()) {
            hashes.push(h);
        }
    };
    let data_id = Regex::new(r#"data-id="([a-f0-9]{16})""#).unwrap();
    for cap in data_id.captures_iter(html) {
        push(cap[1].to_string());
    }
    let skin_link = Regex::new(r#"/skin/([a-f0-9]{16})"#).unwrap();
    for cap in skin_link.captures_iter(html) {
        push(cap[1].to_string());
    }
    hashes
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeSkinResult {
    pub success: bool,
    pub skin_data_base64: Option<String>,
    pub skin_hash: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn scrape_namemc_skin(app: AppHandle, ign: String) -> ScrapeSkinResult {
    let html = match fetch_profile_html(&app, &ign).await {
        Ok(h) => h,
        Err(e) => {
            return ScrapeSkinResult {
                success: false,
                error: Some(e),
                ..Default::default()
            }
        }
    };
    let hashes = extract_hashes(&html);
    if hashes.is_empty() {
        return ScrapeSkinResult {
            success: false,
            error: Some("No skins found on NameMC profile".into()),
            ..Default::default()
        };
    }
    let active_hash = &hashes[0];
    crate::dbg_log!("scrape_namemc_skin: {} hashes, active={}", hashes.len(), active_hash);
    let url = format!("https://s.namemc.com/i/{}.png", active_hash);
    match crate::commands::files::download_bytes(&url).await {
        Ok(bytes) => ScrapeSkinResult {
            success: true,
            skin_data_base64: Some(B64.encode(&bytes)),
            skin_hash: Some(active_hash.clone()),
            ..Default::default()
        },
        Err(e) => ScrapeSkinResult {
            success: false,
            error: Some(e),
            ..Default::default()
        },
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScrapeAllResult {
    pub success: bool,
    pub skins: Vec<String>,
    pub count: usize,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn scrape_namemc_all_skins(app: AppHandle, ign: String) -> ScrapeAllResult {
    let t0 = std::time::Instant::now();
    let html = match wreq_namemc_skins(&ign).await {
        Ok(h) => h,
        Err(e) => {
            crate::dbg_log!("scrape_namemc_all_skins: wreq page failed ({e}), falling back");
            match fetch_profile_html(&app, &ign).await {
                Ok(h) => h,
                Err(e2) => {
                    return ScrapeAllResult {
                        success: false,
                        error: Some(e2),
                        ..Default::default()
                    }
                }
            }
        }
    };
    let hashes: Vec<String> = extract_hashes(&html).into_iter().take(27).collect();
    crate::dbg_log!(
        "scrape_namemc_all_skins: page fetched in {:?}, {} hashes",
        t0.elapsed(),
        hashes.len()
    );
    if hashes.is_empty() {
        return ScrapeAllResult {
            success: false,
            error: Some("No skins found on NameMC profile".into()),
            ..Default::default()
        };
    }

    let t1 = std::time::Instant::now();
    let raw = download_skins_parallel(&hashes).await;
    crate::dbg_log!(
        "scrape_namemc_all_skins: downloaded {}/{} skins in {:?}",
        raw.len(),
        hashes.len(),
        t1.elapsed()
    );

    let mut skins = Vec::new();
    for bytes in raw {
        if let Ok(face) = skin_face_png(&bytes) {
            skins.push(format!("data:image/png;base64,{}", B64.encode(&face)));
        }
    }
    crate::dbg_log!(
        "scrape_namemc_all_skins: {} faces in {:?}",
        skins.len(),
        t0.elapsed()
    );

    if skins.is_empty() {
        return ScrapeAllResult {
            success: false,
            error: Some("Failed to download any skins".into()),
            ..Default::default()
        };
    }

    let count = skins.len();
    ScrapeAllResult {
        success: true,
        skins,
        count,
        error: None,
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UploadResult {
    pub success: bool,
    pub cancelled: bool,
    pub status_code: Option<u16>,
    pub error: Option<String>,
    pub skin_url: Option<String>,
}

#[derive(Default)]
pub struct UploadState {
    pub active: Mutex<Option<Arc<AtomicBool>>>,
}

#[tauri::command]
pub fn cancel_upload(state: State<'_, UploadState>) -> Result<bool, String> {
    let guard = state.active.lock().unwrap();
    if let Some(cancel) = &*guard {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(true)
}

async fn upload_cancel_waiter(cancel: Arc<AtomicBool>) {
    loop {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

#[tauri::command]
pub async fn upload_one_skin(
    state: State<'_, UploadState>,
    bearer_token: String,
    skin_path: String,
    variant: Option<String>,
) -> Result<UploadResult, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.active.lock().unwrap();
        *guard = Some(cancel.clone());
    }
    let result = upload_one_skin_inner(&bearer_token, &skin_path, variant, cancel).await;
    {
        let mut guard = state.active.lock().unwrap();
        *guard = None;
    }
    Ok(result)
}

async fn upload_one_skin_inner(
    bearer_token: &str,
    skin_path: &str,
    variant: Option<String>,
    cancel: Arc<AtomicBool>,
) -> UploadResult {
    let bytes = match fs::read(skin_path) {
        Ok(b) => b,
        Err(e) => {
            return UploadResult {
                success: false,
                error: Some(e.to_string()),
                ..Default::default()
            }
        }
    };
    let part = match reqwest::multipart::Part::bytes(bytes)
        .file_name("skin.png")
        .mime_str("image/png")
    {
        Ok(p) => p,
        Err(e) => {
            return UploadResult {
                success: false,
                error: Some(e.to_string()),
                ..Default::default()
            }
        }
    };
    let form = reqwest::multipart::Form::new()
        .text("variant", variant.unwrap_or_else(|| "classic".into()))
        .part("file", part);

    let client = http_client();
    let request = client
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(bearer_token)
        .multipart(form)
        .send();

    let resp = tokio::select! {
        r = request => match r {
            Ok(resp) => resp,
            Err(e) => {
                return UploadResult {
                    success: false,
                    error: Some(e.to_string()),
                    ..Default::default()
                }
            }
        },
        _ = upload_cancel_waiter(cancel) => {
            return UploadResult {
                success: false,
                cancelled: true,
                error: Some("Upload cancelled".into()),
                ..Default::default()
            }
        }
    };

    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    crate::dbg_log!("upload_one_skin: HTTP {}", status);
    if (200..300).contains(&status) {
        // The upload response is the account profile, which includes the
        // freshly activated skin URL. Its texture hash is the SHA-256 of the
        // exact bytes we uploaded, so the frontend can confirm the upload
        // instantly without polling the (slowly caching) session server.
        let skin_url = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|j| {
                j.get("skins")
                    .and_then(|s| s.as_array())
                    .and_then(|arr| {
                        arr.iter()
                            .find(|s| s.get("state").and_then(|v| v.as_str()) == Some("ACTIVE"))
                    })
                    .and_then(|s| s.get("url").and_then(|u| u.as_str()).map(|u| u.to_string()))
            });
        return UploadResult {
            success: true,
            status_code: Some(status),
            error: None,
            skin_url,
            ..Default::default()
        };
    }

    let error = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|j| {
            j.get("error")
                .cloned()
                .or_else(|| j.get("message").cloned())
                .or_else(|| j.get("errorType").cloned())
        })
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| format!("HTTP {}", status));

    UploadResult {
        success: false,
        status_code: Some(status),
        error: Some(error),
        skin_url: None,
        ..Default::default()
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UuidResult {
    pub uuid: Option<String>,
    pub name: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn get_uuid_from_name(username: String) -> UuidResult {
    let client = http_client();
    let url = format!(
        "https://api.mojang.com/users/profiles/minecraft/{}",
        urlencoding::encode(&username)
    );
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let json: Value = match resp.json().await {
                Ok(j) => j,
                Err(_) => {
                    return UuidResult {
                        error: Some("bad response".into()),
                        ..Default::default()
                    }
                }
            };
            UuidResult {
                uuid: json["id"].as_str().map(|s| s.to_string()),
                name: json["name"].as_str().map(|s| s.to_string()),
                error: None,
            }
        }
        Ok(_) => UuidResult {
            error: Some("profile not found".into()),
            ..Default::default()
        },
        Err(e) => UuidResult {
            error: Some(e.to_string()),
            ..Default::default()
        },
    }
}
