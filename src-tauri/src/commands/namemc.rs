use crate::commands::files::http_client;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use image::ImageReader;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::fs;

fn curl_path() -> &'static str {
    if cfg!(windows) && std::path::Path::new("C:\\Windows\\System32\\curl.exe").exists() {
        "C:\\Windows\\System32\\curl.exe"
    } else {
        "curl"
    }
}

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// True when the response is a Cloudflare interstitial rather than the profile.
fn is_challenge(html: &str) -> bool {
    html.contains("Just a moment")
        || html.contains("cf-chl-")
        || html.contains("challenge-platform")
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
/// we fall back to reqwest with full browser headers.
async fn fetch_profile_html(ign: &str) -> Result<String, String> {
    let url = format!("https://namemc.com/profile/{}", urlencoding::encode(ign));

    // 1. Try curl with browser-like headers.
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

    // 2. Fall back to reqwest with full browser headers and one retry.
    match reqwest_fetch(&url).await {
        Ok(html) if !html.trim().is_empty() => {
            if is_challenge(&html) {
                return Err(
                    "NameMC is showing a Cloudflare challenge. Wait a few seconds and try again."
                        .into(),
                );
            }
            Ok(html)
        }
        Ok(_) => Err("Empty response from NameMC".into()),
        Err(e) => Err(format!("NameMC fetch failed: {}", e)),
    }
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
    let re = Regex::new(r#"data-id="([a-f0-9]{16})""#).unwrap();
    let mut hashes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for cap in re.captures_iter(html) {
        let h = cap[1].to_string();
        if seen.insert(h.clone()) {
            hashes.push(h);
        }
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
pub async fn scrape_namemc_skin(ign: String) -> ScrapeSkinResult {
    let html = match fetch_profile_html(&ign).await {
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

fn face_data_url(skin_png: &[u8]) -> Result<String, String> {
    let img = ImageReader::new(std::io::Cursor::new(skin_png))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let face = image::imageops::crop_imm(&rgba, 8, 8, 8, 8).to_image();
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(face)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(format!("data:image/png;base64,{}", B64.encode(&out)))
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
pub async fn scrape_namemc_all_skins(ign: String) -> ScrapeAllResult {
    let html = match fetch_profile_html(&ign).await {
        Ok(h) => h,
        Err(e) => {
            return ScrapeAllResult {
                success: false,
                error: Some(e),
                ..Default::default()
            }
        }
    };
    let hashes = extract_hashes(&html);
    if hashes.is_empty() {
        return ScrapeAllResult {
            success: false,
            error: Some("No skins found on NameMC profile".into()),
            ..Default::default()
        };
    }

    crate::dbg_log!("scrape_namemc_all_skins: {} hashes", hashes.len());
    let mut skins = Vec::new();
    for hash in hashes.iter().take(27) {
        let url = format!("https://s.namemc.com/i/{}.png", hash);
        if let Ok(bytes) = crate::commands::files::download_bytes(&url).await {
            if let Ok(data_url) = face_data_url(&bytes) {
                skins.push(data_url);
            }
        }
    }

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
    pub status_code: Option<u16>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn upload_one_skin(
    bearer_token: String,
    skin_path: String,
    variant: Option<String>,
) -> UploadResult {
    let bytes = match fs::read(&skin_path) {
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
    let resp = match client
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(&bearer_token)
        .multipart(form)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return UploadResult {
                success: false,
                error: Some(e.to_string()),
                ..Default::default()
            }
        }
    };

    let status = resp.status().as_u16();
    crate::dbg_log!("upload_one_skin: HTTP {}", status);
    if (200..300).contains(&status) {
        return UploadResult {
            success: true,
            status_code: Some(status),
            error: None,
        };
    }

    let text = resp.text().await.unwrap_or_default();
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
