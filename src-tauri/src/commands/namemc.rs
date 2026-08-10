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

/// Fetch the NameMC profile page for an IGN.
///
/// NameMC is behind Cloudflare and blocks programmatic HTTP clients based on
/// their TLS fingerprint (reqwest gets a 403 "Just a moment..." challenge even
/// with browser headers). The Windows system curl.exe (built on SChannel)
/// passes Cloudflare's checks, so we shell out to it instead.
async fn fetch_profile_html(ign: &str) -> Result<String, String> {
    let url = format!("https://namemc.com/profile/{}", urlencoding::encode(ign));
    let output = tokio::process::Command::new(curl_path())
        .arg("-sSL")
        .arg("--compressed")
        .arg("--max-time")
        .arg("25")
        .arg("--retry")
        .arg("2")
        .arg("-A")
        .arg(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .arg(&url)
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
    String::from_utf8(output.stdout).map_err(|e| format!("Bad response from NameMC: {}", e))
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
