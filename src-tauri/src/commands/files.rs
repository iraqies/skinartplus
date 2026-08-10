use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Deserialize;
use std::fs;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Skinart+/1.0")
        .build()
        .unwrap_or_default()
}

pub async fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    let client = http_client();
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    resp.bytes().await.map(|b| b.to_vec()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn select_image(app: AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Images", &["png"])
        .pick_file(move |f| {
            let _ = tx.send(f.map(|p| p.to_string()));
        });
    rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn select_base_skin(app: AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Images", &["png"])
        .pick_file(move |f| {
            let _ = tx.send(f.map(|p| p.to_string()));
        });
    rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn select_original_skin(app: AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .add_filter("Images", &["png"])
        .pick_file(move |f| {
            let _ = tx.send(f.map(|p| p.to_string()));
        });
    rx.await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub skins: Vec<ExportSkin>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSkin {
    pub num: i32,
    pub path: String,
}

#[tauri::command]
pub async fn select_export_dir(
    app: AppHandle,
    opts: ExportRequest,
) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Select Export Folder")
        .pick_folder(move |f| {
            let _ = tx.send(f.map(|p| p.to_string()));
        });
    let dest = rx.await.map_err(|e| e.to_string())?;
    if let Some(dir) = dest {
        for skin in &opts.skins {
            let name = format!("skin_{:02}.png", skin.num);
            let target = std::path::Path::new(&dir).join(&name);
            if let Err(e) = fs::copy(&skin.path, &target) {
                return Err(format!("Failed to copy {}: {}", name, e));
            }
        }
        Ok(Some(dir))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_namemc_cache(app: AppHandle, ign: String) -> Result<(), String> {
    let url = format!(
        "https://namemc.com/profile/{}",
        urlencoding::encode(&ign)
    );
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Base64Result {
    pub success: bool,
    pub data: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn read_file_base64(file_path: String) -> Base64Result {
    match fs::read(&file_path) {
        Ok(bytes) => Base64Result {
            success: true,
            data: Some(B64.encode(&bytes)),
            error: None,
        },
        Err(e) => Base64Result {
            success: false,
            data: None,
            error: Some(e.to_string()),
        },
    }
}

#[tauri::command]
pub fn save_temp_buffer(app: AppHandle, data: String, filename: String) -> Result<String, String> {
    let name = if filename.trim().is_empty() {
        format!("skinartplus_temp_{}.png", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0))
    } else {
        filename
    };
    let dir = app.path().temp_dir().unwrap_or_default();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(&name);
    let bytes = B64.decode(&data).map_err(|e| e.to_string())?;
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

pub fn urlencode(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}
