use super::crypto;
use serde_json::Value;
use std::fs;
use tauri::{AppHandle, Manager};

fn accounts_path(app: &AppHandle) -> std::path::PathBuf {
    app.path().app_data_dir().unwrap_or_default().join("accounts.json")
}

fn read_accounts(app: &AppHandle) -> Vec<Value> {
    let p = accounts_path(app);
    if let Ok(text) = fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(a) = v.as_array() {
                return a.clone();
            }
        }
    }
    vec![]
}

fn write_accounts(app: &AppHandle, accounts: &[Value]) -> Result<(), String> {
    let p = accounts_path(app);
    if let Some(dir) = p.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let json = serde_json::to_string_pretty(accounts).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_accounts(app: AppHandle) -> Result<Vec<Value>, String> {
    let mut accounts = read_accounts(&app);
    for acct in accounts.iter_mut() {
        if let Some(Value::String(rt)) = acct.get_mut("refreshToken") {
            if crypto::is_encrypted(rt) {
                *rt = crypto::decrypt_tokens(rt).unwrap_or_default();
            }
        }
    }
    Ok(accounts)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAccountRequest {
    pub ign: String,
    pub uuid: Option<String>,
    pub refresh_token: Option<String>,
}

#[tauri::command]
pub fn save_account(app: AppHandle, account: SaveAccountRequest) -> Result<Vec<Value>, String> {
    let mut accounts = read_accounts(&app);
    for acct in accounts.iter_mut() {
        acct["lastUsed"] = serde_json::json!(false);
    }
    let mut entry = serde_json::json!({ "ign": account.ign, "lastUsed": true });
    if let Some(uuid) = account.uuid {
        if !uuid.is_empty() {
            entry["uuid"] = Value::String(uuid);
        }
    }
    if let Some(rt) = account.refresh_token {
        if !rt.is_empty() {
            let encrypted = crypto::encrypt_tokens(&rt)?;
            entry["refreshToken"] = Value::String(encrypted);
        }
    }
    let ign = entry["ign"].as_str().unwrap_or("").to_string();
    if let Some(idx) = accounts.iter().position(|a| a["ign"].as_str() == Some(&ign)) {
        let mut merged = accounts[idx].clone();
        if let Some(obj) = merged.as_object_mut() {
            for (k, v) in entry.as_object().unwrap() {
                obj.insert(k.clone(), v.clone());
            }
        }
        accounts[idx] = merged;
    } else {
        accounts.push(entry);
    }
    write_accounts(&app, &accounts)?;
    Ok(accounts)
}

#[tauri::command]
pub fn delete_account(app: AppHandle, ign: String) -> Result<Vec<Value>, String> {
    let mut accounts = read_accounts(&app);
    accounts.retain(|a| a["ign"].as_str() != Some(&ign));
    if !accounts.is_empty() && accounts.iter().all(|a| a["lastUsed"].as_bool().unwrap_or(false) != true) {
        accounts[0]["lastUsed"] = serde_json::json!(true);
    }
    write_accounts(&app, &accounts)?;
    Ok(accounts)
}
