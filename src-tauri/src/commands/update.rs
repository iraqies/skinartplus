use serde::Deserialize;
use tauri_plugin_opener::OpenerExt;

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub is_outdated: bool,
    pub release_url: String,
    pub download_url: Option<String>,
}

fn current_version(app: &tauri::AppHandle) -> String {
    app.package_info()
        .version
        .to_string()
        .trim_start_matches('v')
        .to_string()
}

fn parse_version(v: &str) -> (u64, u64, u64) {
    let cleaned = v.trim_start_matches('v');
    let parts: Vec<&str> = cleaned.split('.').collect();
    let get = |i: usize| parts.get(i).and_then(|p| p.parse::<u64>().ok()).unwrap_or(0);
    (get(0), get(1), get(2))
}

fn is_newer(latest: &str, current: &str) -> bool {
    let a = parse_version(latest);
    let b = parse_version(current);
    a > b
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current = current_version(&app);
    let url = "https://api.github.com/repos/iraqies/skinartplus/releases/latest";

    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header("User-Agent", "SkinartPlus-Updater")
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Ok(UpdateInfo {
            current_version: current.clone(),
            latest_version: current,
            is_outdated: false,
            release_url: String::new(),
            download_url: None,
        });
    }

    let release: GitHubRelease = resp.json().await.map_err(|e| e.to_string())?;
    let latest = release.tag_name.trim_start_matches('v').to_string();

    let download_url = release
        .assets
        .iter()
        .find(|a| {
            let n = a.name.to_lowercase();
            n.contains("setup") || n.ends_with(".exe") || n.ends_with(".msi")
        })
        .map(|a| a.browser_download_url.clone());

    Ok(UpdateInfo {
        current_version: current.clone(),
        latest_version: latest.clone(),
        is_outdated: is_newer(&latest, &current),
        release_url: release.html_url,
        download_url,
    })
}

#[tauri::command]
pub fn get_app_version(app: tauri::AppHandle) -> String {
    current_version(&app)
}

#[tauri::command]
pub async fn open_latest_release(app: tauri::AppHandle) -> Result<(), String> {
    let info = check_for_update(app.clone()).await?;
    if info.is_outdated && !info.release_url.is_empty() {
        app.opener()
            .open_url(&info.release_url, None::<&str>)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
