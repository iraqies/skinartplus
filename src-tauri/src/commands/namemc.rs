use crate::commands::files::http_client;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::State;
use tauri::{AppHandle, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

const NAMEMC_SCRAPE_EVENT: &str = "namemc-scrape-result";

const SCRAPE_SCRIPT: &str = r#"
(function () {
    if (!window.__TAURI__ || !window.__TAURI__.event) return;
    var sent = false;
    var clearedAt = 0;
    function tick() {
        if (sent) return;
        var html = document.documentElement ? document.documentElement.outerHTML : '';
        var title = document.title || '';
        var token = (location.hash || '').replace(/^#/, '');
        var onNameMc = location.hostname === 'namemc.com'
            && (location.pathname.indexOf('/profile/') === 0 || location.pathname.indexOf('/search') === 0);
        if (!html || !onNameMc || token.indexOf('scrape=') === -1) return;
        if (title.indexOf('Just a moment') !== -1 || html.indexOf('Just a moment') !== -1) return;
        var hasSkins = html.indexOf('data-id="') !== -1;
        if (!hasSkins) {
            if (!clearedAt) clearedAt = Date.now();
            if (Date.now() - clearedAt < 3000) return;
        }
        sent = true;
        window.__TAURI__.event.emit('namemc-scrape-result', {
            token: token,
            html: html,
            url: location.href
        });
    }
    setInterval(tick, 400);
})();
"#;

fn scrape_gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn next_scrape_token() -> u64 {
    static GEN: AtomicU64 = AtomicU64::new(1);
    GEN.fetch_add(1, Ordering::Relaxed)
}

struct ScrapedPage {
    html: String,
    url: String,
}

/// Open `https://namemc.com/profile/<name>` in the app WebView and return that page.
///
/// Cloudflare blocks plain requests to this URL. The window stays hidden, so
/// the check can finish and NameMC can redirect to the canonical profile.
/// No other site is requested for the page itself.
async fn fetch_profile_html(app: &AppHandle, ign: &str) -> Result<String, String> {
    let _gate = scrape_gate().lock().await;
    let generation = next_scrape_token();
    let token = format!("scrape={generation}");
    let profile_url = format!(
        "https://namemc.com/profile/{}#{}",
        urlencoding::encode(ign),
        token
    );

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ScrapedPage>();
    let expected = token.clone();
    let listener_id = app.listen(NAMEMC_SCRAPE_EVENT, move |event| {
        let Ok(value) = serde_json::from_str::<Value>(event.payload()) else {
            return;
        };
        if value.get("token").and_then(|item| item.as_str()) != Some(expected.as_str()) {
            return;
        }
        let Some(html) = value.get("html").and_then(|item| item.as_str()) else {
            return;
        };
        if html.is_empty() {
            return;
        }
        let url = value
            .get("url")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .to_string();
        let _ = tx.send(ScrapedPage {
            html: html.to_string(),
            url,
        });
    });

    if let Err(err) = open_scrape_window(app, &profile_url) {
        app.unlisten(listener_id);
        return Err(err);
    }

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut result = Err(
        "Timed out waiting for NameMC in the background browser. The page may be stuck on a Cloudflare check — try again in a few seconds."
            .into(),
    );
    while tokio::time::Instant::now() < deadline {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Some(page)) => {
                if !page.url.contains("namemc.com/profile/")
                    && !page.url.contains("namemc.com/search")
                    && !page.html.contains("Search | NameMC")
                {
                    continue;
                }
                result = Ok(page.html);
                break;
            }
            Ok(None) => {
                result = Err("NameMC scraper was interrupted".into());
                break;
            }
            Err(_) => break,
        }
    }

    app.unlisten(listener_id);
    result
}

fn open_scrape_window(app: &AppHandle, url: &str) -> Result<(), String> {
    let url = tauri::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    if let Some(window) = app.get_webview_window("namemc-scrape") {
        let _ = window.hide();
        window
            .navigate(url)
            .map_err(|e| format!("Failed to open the NameMC profile: {e}"))?;
        return Ok(());
    }

    WebviewWindowBuilder::new(app, "namemc-scrape", WebviewUrl::External(url))
        .title("NameMC")
        .inner_size(1100.0, 820.0)
        .visible(false)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .initialization_script(SCRAPE_SCRIPT)
        .build()
        .map_err(|e| format!("Failed to open NameMC in the background browser: {e}"))?;
    Ok(())
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
    let html = match fetch_profile_html(&app, &ign).await {
        Ok(h) => h,
        Err(e) => {
            return ScrapeAllResult {
                success: false,
                error: Some(e),
                ..Default::default()
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
