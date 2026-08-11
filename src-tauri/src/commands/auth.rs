use crate::commands::files::{http_client, urlencode};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use qrcode::render::svg;
use qrcode::QrCode;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};

const MC_CLIENT_ID: &str = env!("SKINARTPLUS_CLIENT_ID");
const MC_CLIENT_SECRET: Option<&str> = option_env!("SKINARTPLUS_CLIENT_SECRET");
const MS_AUTHORITY: &str = env!("SKINARTPLUS_MS_AUTHORITY");
const REDIRECT_URI: &str = env!("SKINARTPLUS_REDIRECT_URI");

#[derive(Default)]
struct AuthCodeState {
    code: Option<String>,
    error: Option<String>,
    completed: Option<PollResult>,
}

const CALLBACK_SUCCESS_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Skinart+ - Signed in</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: 'Segoe UI', system-ui, sans-serif; background: radial-gradient(1200px 600px at 20% 0%, rgba(20,184,166,0.06), transparent 50%), radial-gradient(1000px 500px at 80% 100%, rgba(20,184,166,0.04), transparent 50%), #0f172a; color: #e2e8f0; min-height: 100vh; display: flex; align-items: center; justify-content: center; }
  .card { background: #1e293b; border: 1px solid #334155; border-radius: 16px; padding: 44px 52px; text-align: center; box-shadow: 0 8px 32px rgba(0,0,0,0.4); max-width: 400px; }
  .check { width: 64px; height: 64px; margin: 0 auto 18px; border-radius: 50%; background: rgba(20,184,166,0.15); display: flex; align-items: center; justify-content: center; }
  h1 { font-size: 20px; margin-bottom: 8px; }
  p { color: #94a3b8; font-size: 14px; line-height: 1.5; }
</style>
</head>
<body>
  <div class="card">
    <div class="check"><svg viewBox="0 0 24 24" fill="none"><path d="M5 13l4 4L19 7" stroke="#2dd4bf" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/></svg></div>
    <h1>Signed in</h1>
    <p>You can now close this tab and return to Skinart+.</p>
  </div>
</body>
</html>"##;

const CALLBACK_ERROR_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Skinart+ - Sign-in failed</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: 'Segoe UI', system-ui, sans-serif; background: #0f172a; color: #e2e8f0; min-height: 100vh; display: flex; align-items: center; justify-content: center; }
  .card { background: #1e293b; border: 1px solid #334155; border-radius: 16px; padding: 44px 52px; text-align: center; box-shadow: 0 8px 32px rgba(0,0,0,0.4); max-width: 400px; }
  .check { width: 64px; height: 64px; margin: 0 auto 18px; border-radius: 50%; background: rgba(244,63,94,0.15); display: flex; align-items: center; justify-content: center; }
  h1 { font-size: 20px; margin-bottom: 8px; }
  p { color: #94a3b8; font-size: 14px; line-height: 1.5; }
</style>
</head>
<body>
  <div class="card">
    <div class="check"><svg viewBox="0 0 24 24" fill="none"><path d="M6 6l12 12M18 6L6 18" stroke="#fb7185" stroke-width="2.5" stroke-linecap="round"/></svg></div>
    <h1>Sign-in failed</h1>
    <p>Something went wrong. Close this tab and try again in Skinart+.</p>
  </div>
</body>
</html>"##;

fn auth_code_state() -> &'static Mutex<AuthCodeState> {
    static STATE: OnceLock<Mutex<AuthCodeState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(AuthCodeState::default()))
}

fn redirect_host_port() -> (String, u16) {
    let rest = REDIRECT_URI.strip_prefix("http://").unwrap_or(REDIRECT_URI);
    let (host_port, _path) = rest.split_once('/').unwrap_or((rest, ""));
    match host_port.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().unwrap_or(80)),
        None => (host_port.to_string(), 80),
    }
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn http_respond(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = if status == 200 { "OK" } else { "Bad Request" };
    let resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        reason,
        body.len(),
        body
    );
    stream.write_all(resp.as_bytes())
}

fn spawn_code_callback_server() -> Result<(), String> {
    let (host, port) = redirect_host_port();
    let listener = TcpListener::bind((host.as_str(), port))
        .map_err(|e| format!("Cannot start local callback server on {}:{}: {}", host, port, e))?;
    {
        let mut state = auth_code_state().lock().unwrap();
        *state = AuthCodeState::default();
    }
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let first_line = request.lines().next().unwrap_or("").to_string();
            let path = first_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .to_string();
            let mut state = auth_code_state().lock().unwrap();
            if let Some((_, query)) = path.split_once('?') {
                let params: HashMap<String, String> = query
                    .split('&')
                    .filter_map(|kv| {
                        let mut it = kv.splitn(2, '=');
                        Some((urldecode(it.next()?), urldecode(it.next().unwrap_or(""))))
                    })
                    .collect();
                if let Some(code) = params.get("code") {
                    state.code = Some(code.clone());
                    let _ = http_respond(&mut stream, 200, CALLBACK_SUCCESS_HTML);
                } else if let Some(err) = params
                    .get("error_description")
                    .or_else(|| params.get("error"))
                {
                    state.error = Some(err.clone());
                    let _ = http_respond(&mut stream, 200, CALLBACK_ERROR_HTML);
                } else {
                    state.error = Some("No authorization code received".into());
                    let _ = http_respond(&mut stream, 400, "Missing code");
                }
            } else {
                state.error = Some("Invalid callback request".into());
                let _ = http_respond(&mut stream, 400, "Bad request");
            }
            break;
        }
    });
    Ok(())
}

fn form_body(params: &[(&str, &str)]) -> String {
    let mut parts: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect();
    if let Some(secret) = MC_CLIENT_SECRET {
        parts.push(format!("client_secret={}", urlencode(secret)));
    }
    parts.join("&")
}

async fn ms_token(body: String) -> Result<Value, String> {
    let client = http_client();
    let resp = client
        .post(format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            MS_AUTHORITY
        ))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

async fn post_json(url: &str, body: &Value) -> Result<Value, String> {
    let client = http_client();
    let resp = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json::<Value>().await.map_err(|e| e.to_string())
}

async fn exchange_for_minecraft(ms_token_value: &str) -> Result<String, String> {
    let xbl = post_json(
        "https://user.auth.xboxlive.com/user/authenticate",
        &serde_json::json!({
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={}", ms_token_value)
            }
        }),
    )
    .await?;
    if xbl.get("error").is_some() || xbl.get("Token").is_none() {
        return Err(xbl
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or("Xbox Live auth failed")
            .to_string());
    }
    let xbl_token = xbl["Token"].as_str().unwrap_or("").to_string();
    let uhs = xbl["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .unwrap_or("")
        .to_string();

    let xsts = post_json(
        "https://xsts.auth.xboxlive.com/xsts/authorize",
        &serde_json::json!({
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl_token]
            }
        }),
    )
    .await?;
    if xsts.get("error").is_some() || xsts.get("Token").is_none() {
        return Err(xsts
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or("XSTS auth failed")
            .to_string());
    }
    let xsts_token = xsts["Token"].as_str().unwrap_or("").to_string();

    let mc = post_json(
        "https://api.minecraftservices.com/authentication/login_with_xbox",
        &serde_json::json!({
            "identityToken": format!("XBL3.0 x={};{}", uhs, xsts_token)
        }),
    )
    .await?;
    let access = mc["access_token"]
        .as_str()
        .ok_or_else(|| "Minecraft auth failed: no access_token".to_string())?;
    Ok(access.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStartResult {
    pub flow: String,
    pub verification_uri: String,
    pub user_code: String,
    pub device_code: String,
    pub interval: i64,
}

/// "code" when the private app (client secret) is compiled in, "device" otherwise.
#[tauri::command]
pub fn auth_flow_mode() -> String {
    if MC_CLIENT_SECRET.is_some() {
        "code".into()
    } else {
        "device".into()
    }
}

/// Render a QR code as an SVG data URL the frontend can drop into an <img>.
#[tauri::command]
pub fn auth_qr(content: String) -> Result<String, String> {
    if content.is_empty() {
        return Err("Nothing to encode".into());
    }
    let code = QrCode::with_error_correction_level(content.as_bytes(), qrcode::EcLevel::M)
        .map_err(|e| e.to_string())?;
    let svg_str = code
        .render::<svg::Color>()
        .min_dimensions(200, 200)
        .quiet_zone(true)
        .build();
    Ok(format!("data:image/svg+xml;base64,{}", B64.encode(svg_str.as_bytes())))
}

#[tauri::command]
pub async fn start_auth() -> Result<AuthStartResult, String> {
    if MC_CLIENT_SECRET.is_some() {
        crate::dbg_log!("start_auth: code flow");
        spawn_code_callback_server()?;
        let url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&scope={}&prompt=select_account",
            MS_AUTHORITY,
            urlencode(MC_CLIENT_ID),
            urlencode(REDIRECT_URI),
            urlencode("XboxLive.signin offline_access")
        );
        return Ok(AuthStartResult {
            flow: "code".into(),
            verification_uri: url,
            user_code: String::new(),
            device_code: String::new(),
            interval: 2,
        });
    }

    let body = form_body(&[
        ("client_id", MC_CLIENT_ID),
        ("scope", "XboxLive.signin offline_access"),
    ]);
    let client = http_client();
    crate::dbg_log!("start_auth: device flow");
    let resp = client
        .post(format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode",
            MS_AUTHORITY
        ))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let json: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if json.get("error").is_some() {
        return Err(json
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or("device code error")
            .to_string());
    }
    Ok(AuthStartResult {
        flow: "device".into(),
        verification_uri: json["verification_uri"].as_str().unwrap_or("").to_string(),
        user_code: json["user_code"].as_str().unwrap_or("").to_string(),
        device_code: json["device_code"].as_str().unwrap_or("").to_string(),
        interval: json["interval"].as_i64().unwrap_or(5),
    })
}

#[derive(Serialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PollResult {
    pub status: String,
    pub bearer_token: Option<String>,
    pub refresh_token: Option<String>,
    pub message: Option<String>,
}

#[tauri::command]
pub async fn poll_auth_token(device_code: String) -> PollResult {
    let body = form_body(&[
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("client_id", MC_CLIENT_ID),
        ("device_code", device_code.as_str()),
    ]);
    match ms_token(body).await {
        Ok(json) => {
            if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
                return match err {
                    "authorization_pending" => PollResult {
                        status: "pending".into(),
                        ..Default::default()
                    },
                    "slow_down" => PollResult {
                        status: "slow_down".into(),
                        ..Default::default()
                    },
                    _ => PollResult {
                        status: "error".into(),
                        message: Some(
                            json.get("error_description")
                                .and_then(|v| v.as_str())
                                .unwrap_or(err)
                                .to_string(),
                        ),
                        ..Default::default()
                    },
                };
            }
            let access = json["access_token"].as_str().unwrap_or("");
            crate::dbg_log!("device poll: token received, exchanging for minecraft...");
            match exchange_for_minecraft(access).await {
                Ok(bearer) => PollResult {
                    status: "success".into(),
                    bearer_token: Some(bearer),
                    refresh_token: json
                        .get("refresh_token")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    ..Default::default()
                },
                Err(e) => PollResult {
                    status: "error".into(),
                    message: Some(e),
                    ..Default::default()
                },
            }
        }
        Err(e) => PollResult {
            status: "error".into(),
            message: Some(e),
            ..Default::default()
        },
    }
}

#[tauri::command]
pub async fn poll_auth_code() -> PollResult {
    let (code, error) = {
        let mut state = auth_code_state().lock().unwrap();
        if let Some(done) = state.completed.clone() {
            return done;
        }
        (state.code.take(), state.error.clone())
    };
    let Some(code) = code else {
        return PollResult {
            status: if error.is_some() { "error" } else { "pending" }.into(),
            message: error,
            ..Default::default()
        };
    };
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("client_id", MC_CLIENT_ID),
        ("code", code.as_str()),
        ("redirect_uri", REDIRECT_URI),
    ]);
    let result = match ms_token(body).await {
        Ok(json) => {
            if let Some(err) = json.get("error").and_then(|v| v.as_str()) {
                PollResult {
                    status: "error".into(),
                    message: Some(
                        json.get("error_description")
                            .and_then(|v| v.as_str())
                            .unwrap_or(err)
                            .to_string(),
                    ),
                    ..Default::default()
                }
            } else {
                let access = json["access_token"].as_str().unwrap_or("");
                match exchange_for_minecraft(access).await {
                    Ok(bearer) => PollResult {
                        status: "success".into(),
                        bearer_token: Some(bearer),
                        refresh_token: json
                            .get("refresh_token")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        ..Default::default()
                    },
                    Err(e) => PollResult {
                        status: "error".into(),
                        message: Some(e),
                        ..Default::default()
                    },
                }
            }
        }
        Err(e) => PollResult {
            status: "error".into(),
            message: Some(e),
            ..Default::default()
        },
    };
    {
        let mut state = auth_code_state().lock().unwrap();
        state.code = None;
        state.error = None;
        state.completed = Some(result.clone());
    }
    result
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub success: bool,
    pub bearer_token: Option<String>,
    pub refresh_token: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn refresh_saved_token(refresh_token: String) -> RefreshResult {
    let body = form_body(&[
        ("client_id", MC_CLIENT_ID),
        ("scope", "XboxLive.signin offline_access"),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token.as_str()),
    ]);
    match ms_token(body).await {
        Ok(json) => {
            if json.get("error").is_some() {
                return RefreshResult {
                    success: false,
                    error: Some(
                        json.get("error_description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("refresh failed")
                            .to_string(),
                    ),
                    ..Default::default()
                };
            }
            let access = json["access_token"].as_str().unwrap_or("");
            match exchange_for_minecraft(access).await {
                Ok(bearer) => RefreshResult {
                    success: true,
                    bearer_token: Some(bearer),
                    refresh_token: Some(
                        json.get("refresh_token")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&refresh_token)
                            .to_string(),
                    ),
                    ..Default::default()
                },
                Err(e) => RefreshResult {
                    success: false,
                    error: Some(e),
                    ..Default::default()
                },
            }
        }
        Err(e) => RefreshResult {
            success: false,
            error: Some(e),
            ..Default::default()
        },
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResult {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn fetch_profile(bearer_token: String) -> Result<ProfileResult, String> {
    let client = http_client();
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(&bearer_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(ProfileResult {
        id: json["id"].as_str().unwrap_or("").to_string(),
        name: json["name"].as_str().unwrap_or("").to_string(),
    })
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkinTextureResult {
    pub success: bool,
    pub data: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn download_skin_texture(uuid: String) -> SkinTextureResult {
    let client = http_client();
    let resp = match client
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{}",
            uuid
        ))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return SkinTextureResult {
                success: false,
                error: Some(e.to_string()),
                ..Default::default()
            }
        }
    };
    if !resp.status().is_success() {
        return SkinTextureResult {
            success: false,
            error: Some(format!("HTTP {}", resp.status().as_u16())),
            ..Default::default()
        };
    }
    let json: Value = match resp.json().await {
        Ok(j) => j,
        Err(e) => {
            return SkinTextureResult {
                success: false,
                error: Some(e.to_string()),
                ..Default::default()
            }
        }
    };
    let props = json["properties"].as_array().cloned().unwrap_or_default();
    let texture_prop = props
        .iter()
        .find(|p| p["name"].as_str() == Some("textures"))
        .cloned();
    let Some(tp) = texture_prop else {
        return SkinTextureResult {
            success: false,
            error: Some("No texture data".into()),
            ..Default::default()
        };
    };
    let value = tp["value"].as_str().unwrap_or("");
    let decoded: Value = match B64
        .decode(value)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
    {
        Some(v) => v,
        None => {
            return SkinTextureResult {
                success: false,
                error: Some("Bad texture data".into()),
                ..Default::default()
            }
        }
    };
    let url = decoded["textures"]["SKIN"]["url"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if url.is_empty() {
        return SkinTextureResult {
            success: false,
            error: Some("No skin url".into()),
            ..Default::default()
        };
    }
    match crate::commands::files::download_bytes(&url).await {
        Ok(bytes) => SkinTextureResult {
            success: true,
            data: Some(B64.encode(&bytes)),
            ..Default::default()
        },
        Err(e) => SkinTextureResult {
            success: false,
            error: Some(e),
            ..Default::default()
        },
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AvatarResult {
    pub success: bool,
    pub data_url: Option<String>,
}

#[tauri::command]
pub async fn fetch_avatar(id: String) -> AvatarResult {
    let urls = [
        format!("https://api.mcskin.me/head/{}?size=128", urlencode(&id)),
        format!("https://mc-heads.net/avatar/{}/128", urlencode(&id)),
    ];
    for url in urls {
        if let Ok(bytes) = crate::commands::files::download_bytes(&url).await {
            if bytes.len() > 100 {
                return AvatarResult {
                    success: true,
                    data_url: Some(format!("data:image/png;base64,{}", B64.encode(&bytes))),
                };
            }
        }
    }
    AvatarResult {
        success: false,
        data_url: None,
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HeadResult {
    pub success: bool,
    pub data: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn download_head(uuid: String) -> HeadResult {
    let url = format!("https://mc-heads.net/avatar/{}/128", uuid);
    match crate::commands::files::download_bytes(&url).await {
        Ok(bytes) => HeadResult {
            success: true,
            data: Some(B64.encode(&bytes)),
            ..Default::default()
        },
        Err(e) => HeadResult {
            success: false,
            error: Some(e),
            ..Default::default()
        },
    }
}
