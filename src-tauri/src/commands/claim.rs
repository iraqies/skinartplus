use crate::mc::client;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct ClaimState {
    pub active: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimStatusPayload {
    pub message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimProfile {
    pub name: String,
    pub id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRequest {
    pub bearer_token: String,
    pub profile: ClaimProfile,
    pub server: Option<String>,
    pub port: Option<u16>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimResult {
    pub success: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn claim_namemc(
    app: AppHandle,
    state: State<'_, ClaimState>,
    request: ClaimRequest,
) -> Result<ClaimResult, String> {
    {
        let guard = state.active.lock().unwrap();
        if guard.is_some() {
            return Ok(ClaimResult {
                success: false,
                error: Some("A claim is already in progress".into()),
                url: None,
            });
        }
    }

    let server = request.server.unwrap_or_else(|| "blockmania.com".into());
    let port = request.port.unwrap_or(25565);
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.active.lock().unwrap();
        *guard = Some(cancel.clone());
    }

    let cfg = client::ClaimConfig {
        bearer_token: request.bearer_token.clone(),
        username: request.profile.name.clone(),
        uuid: request.profile.id.clone(),
        server,
        port,
    };

    let emit_app = app.clone();
    let result = client::run_claim(&cfg, cancel.clone(), move |msg| {
        let _ = emit_app.emit(
            "claim-status",
            ClaimStatusPayload {
                message: msg.to_string(),
            },
        );
    })
    .await;

    {
        let mut guard = state.active.lock().unwrap();
        *guard = None;
    }

    match result {
        Ok(url) => Ok(ClaimResult {
            success: true,
            url: Some(url),
            error: None,
        }),
        Err(e) => Ok(ClaimResult {
            success: false,
            url: None,
            error: Some(e),
        }),
    }
}

#[tauri::command]
pub fn cancel_claim(state: State<'_, ClaimState>) -> Result<bool, String> {
    let guard = state.active.lock().unwrap();
    if let Some(cancel) = &*guard {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(true)
}
