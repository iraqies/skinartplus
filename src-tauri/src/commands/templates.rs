use crate::commands::files::http_client;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const GITHUB_RAW: &str = "https://raw.githubusercontent.com/iraqies/MySkinArt/main/templates";

fn app_templates_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_default().join("templates")
}

fn bundled_templates_dir(app: &AppHandle) -> PathBuf {
    app.path().resource_dir().unwrap_or_default().join("templates")
}

fn templates_json_path(dir: &PathBuf) -> PathBuf {
    dir.join("templates.json")
}

fn read_templates(app: &AppHandle) -> Result<Vec<Value>, String> {
    let app_dir = app_templates_dir(app);
    let bundled_dir = bundled_templates_dir(app);
    let candidates = [
        templates_json_path(&app_dir),
        templates_json_path(&bundled_dir),
    ];
    for p in candidates {
        if p.exists() {
            if let Ok(text) = fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(arr) = v.get("templates").and_then(|t| t.as_array()) {
                        return Ok(arr.clone());
                    }
                }
            }
        }
    }
    Ok(vec![])
}

fn resolve_template_file(app: &AppHandle, id: &str) -> Option<PathBuf> {
    let templates = read_templates(app).ok()?;
    let t = templates.iter().find(|t| t["id"].as_str() == Some(id))?;
    let filename = t["filename"].as_str()?;
    let base = Path::new(filename).file_name()?.to_str()?.to_string();
    for dir in [app_templates_dir(app), bundled_templates_dir(app)] {
        for cand in [
            dir.join(filename),
            dir.join(&base),
            dir.join("images").join(&base),
        ] {
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let target = dst.join(&name);
            if path.is_dir() {
                let _ = fs::create_dir_all(&target);
                copy_dir_recursive(&path, &target);
            } else if path.is_file() && !target.exists() {
                let _ = fs::copy(&path, &target);
            }
        }
    }
}

#[tauri::command]
pub fn load_templates(app: AppHandle) -> Result<Vec<Value>, String> {
    read_templates(&app)
}

#[tauri::command]
pub fn get_template_image_path(app: AppHandle, id: String) -> Result<Option<String>, String> {
    Ok(resolve_template_file(&app, &id).map(|p| p.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn get_template_image_data(app: AppHandle, id: String) -> Result<Option<String>, String> {
    let Some(p) = resolve_template_file(&app, &id) else {
        return Ok(None);
    };
    match fs::read(&p) {
        Ok(bytes) => Ok(Some(B64.encode(&bytes))),
        Err(_) => Ok(None),
    }
}

pub fn sync_bundled(app: &AppHandle) -> Result<(), String> {
    let app_dir = app_templates_dir(app);
    let res_dir = bundled_templates_dir(app);
    if !res_dir.exists() {
        return Ok(());
    }
    fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;

    // copy bundled templates.json if the app copy is missing
    let src_json = templates_json_path(&res_dir);
    let dst_json = templates_json_path(&app_dir);
    if !dst_json.exists() && src_json.exists() {
        let _ = fs::copy(&src_json, &dst_json);
    }

    // copy bundled template images if missing
    copy_dir_recursive(&res_dir, &app_dir);
    Ok(())
}

pub async fn sync_remote(app: AppHandle) {
    let client = http_client();
    let app_dir = app_templates_dir(&app);

    let text = match client.get(format!("{}/templates.json", GITHUB_RAW)).send().await {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(_) => return,
    };
    if text.is_empty() {
        return;
    }
    let json: Value = match serde_json::from_str(&text) {
        Ok(j) => j,
        Err(_) => return,
    };
    let _ = fs::create_dir_all(&app_dir);
    let _ = fs::write(templates_json_path(&app_dir), &text);

    if let Some(arr) = json.get("templates").and_then(|t| t.as_array()) {
        for t in arr {
            let Some(fname) = t["filename"].as_str() else {
                continue;
            };
            let dst = app_dir.join(fname);
            if !dst.exists() {
                let url = format!("{}/{}", GITHUB_RAW, fname);
                if let Ok(resp) = client.get(&url).send().await {
                    if let Ok(bytes) = resp.bytes().await {
                        let _ = fs::write(&dst, bytes);
                    }
                }
            }
        }
    }
}
