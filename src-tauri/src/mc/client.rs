// Minimal hand-written Minecraft Java client used for the NameMC claim feature.
// Scope: protocol versions 759-763 (1.19 - 1.20.1).

use crate::mc::codec::{parse_string, parse_varint, varint_bytes, McStream};
use crate::mc::crypto;
use regex::Regex;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;

pub struct ClaimConfig {
    pub bearer_token: String,
    pub username: String,
    pub uuid: String,
    pub server: String,
    pub port: u16,
}

#[derive(Clone, Copy)]
pub enum Layout {
    V759,
    V761,
    V762,
    V763,
}

#[derive(Clone, Copy)]
pub enum ChatStyle {
    V759,
    V761,
    V763,
}

impl Layout {
    pub fn for_protocol(p: i32) -> Option<Layout> {
        match p {
            759 => Some(Layout::V759),
            760 | 761 => Some(Layout::V761),
            762 => Some(Layout::V762),
            763 => Some(Layout::V763),
            _ => None,
        }
    }

    pub fn protocol(self) -> i32 {
        match self {
            Layout::V759 => 759,
            Layout::V761 => 761,
            Layout::V762 => 762,
            Layout::V763 => 763,
        }
    }

    /// clientbound play: keep alive
    pub fn client_keepalive_id(self) -> i32 {
        match self {
            Layout::V759 => 0x1E,
            Layout::V761 => 0x20,
            Layout::V762 => 0x23,
            Layout::V763 => 0x23,
        }
    }

    /// serverbound play: keep alive
    pub fn server_keepalive_id(self) -> i32 {
        match self {
            Layout::V759 => 0x11,
            Layout::V761 => 0x12,
            Layout::V762 => 0x12,
            Layout::V763 => 0x12,
        }
    }

    /// serverbound play: chat message
    pub fn server_chat_id(self) -> i32 {
        match self {
            Layout::V759 => 0x04,
            _ => 0x05,
        }
    }

    /// clientbound play: kick/disconnect
    pub fn client_kick_id(self) -> i32 {
        match self {
            Layout::V759 => 0x17,
            Layout::V761 => 0x19,
            Layout::V762 => 0x1A,
            Layout::V763 => 0x1A,
        }
    }

    pub fn chat_style(self) -> ChatStyle {
        match self {
            Layout::V759 => ChatStyle::V759,
            Layout::V761 => ChatStyle::V761,
            Layout::V762 | Layout::V763 => ChatStyle::V763,
        }
    }
}

/// Detect the protocol version of a server via a status ping.
///
/// Some servers (e.g. blockmania) never answer status pings; a read without a
/// timeout would hang the login forever. We bound the whole ping to a few
/// seconds and fall back to `None` so callers can use a default version.
pub async fn detect_protocol(server: &str, port: u16) -> Option<i32> {
    let stream = TcpStream::connect((server, port)).await.ok()?;
    let mut mc = McStream::new(stream);
    let mut payload = Vec::new();
    payload.extend(varint_bytes(763));
    payload.extend(varint_bytes(server.len() as i32));
    payload.extend_from_slice(server.as_bytes());
    payload.extend_from_slice(&port.to_be_bytes());
    payload.extend(varint_bytes(1)); // next state: status
    if mc.write_packet(0x00, &payload).await.is_err() {
        return None;
    }
    if mc.write_packet(0x00, &[]).await.is_err() {
        return None;
    }
    let (id, data) = match tokio::time::timeout(Duration::from_secs(4), mc.read_packet()).await {
        Ok(Ok(v)) => v,
        _ => return None,
    };
    if id != 0x00 {
        return None;
    }
    let (json_str, _) = match parse_string(&data) {
        Ok(v) => v,
        Err(_) => return None,
    };
    let json: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return None,
    };
    json.get("version")?.get("protocol")?.as_i64().map(|p| p as i32)
}

async fn send_chat(mc: &mut McStream, layout: Layout, message: &str) -> Result<(), String> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let mut body = Vec::new();
    body.extend(varint_bytes(message.len() as i32));
    body.extend_from_slice(message.as_bytes());
    body.extend_from_slice(&now_ms.to_be_bytes());
    body.extend_from_slice(&0i64.to_be_bytes()); // salt

    match layout.chat_style() {
        ChatStyle::V759 => {
            // signature buffer (empty), signedPreview=false
            body.extend(varint_bytes(0));
            body.push(0x00);
        }
        ChatStyle::V761 => {
            // signature buffer (empty), signedPreview=false, previousMessages(empty), lastRejectedMessage(absent)
            body.extend(varint_bytes(0));
            body.push(0x00);
            body.extend(varint_bytes(0));
            body.push(0x00);
        }
        ChatStyle::V763 => {
            // signature option (absent), offset=0, acknowledged buffer (3 zero bytes)
            body.push(0x00);
            body.extend(varint_bytes(0));
            body.extend_from_slice(&[0x00, 0x00, 0x00]);
        }
    }
    mc.write_packet(layout.server_chat_id(), &body).await
}

fn clean_url(url: &str) -> String {
    let allowed = ":;/?!@&=+$,#%._~-";
    let trimmed = url.trim_end_matches(|c: char| !c.is_alphanumeric() && !allowed.contains(c));
    trimmed.to_string()
}

enum LoginOutcome {
    Play(McStream, Layout),
    Disconnected(String),
    Rejected,
    Failed(String),
}

fn version_label(proto: i32) -> &'static str {
    match proto {
        759 => "1.19",
        760 => "1.19.1/1.19.2",
        761 => "1.19.3",
        762 => "1.19.4",
        763 => "1.20/1.20.1",
        _ => "unknown",
    }
}

pub async fn run_claim(
    cfg: &ClaimConfig,
    cancel: Arc<AtomicBool>,
    emit: impl Fn(&str),
) -> Result<String, String> {
    emit("Connecting to server...");

    // 1. detect protocol version (best effort — some proxies advertise one
    //    version in the status ping but only accept logins on another).
    let detected = detect_protocol(&cfg.server, cfg.port).await;
    crate::dbg_log!("claim: detected protocol {:?}", detected);

    // 2. try the detected/default version first, then the versions most
    //    proxies actually accept. blockmania answers status with 763 but
    //    closes the connection on any 761+ login and only accepts 759/760.
    //
    //    Note: BungeeCord proxies throttle reconnects from the same IP
    //    (~4s), so repeated back-to-back attempts all get silently closed.
    //    A rejected attempt is therefore followed by a pause before the
    //    next candidate.
    let mut candidates: Vec<i32> = Vec::new();
    for p in [detected.unwrap_or(763), 760, 759, 762, 761] {
        if !candidates.contains(&p) {
            candidates.push(p);
        }
    }

    for proto in candidates {
        if Layout::for_protocol(proto).is_none() {
            continue;
        }
        emit(&format!("Connecting as {}...", version_label(proto)));
        match attempt_login(cfg, proto, cancel.clone(), &emit).await {
            LoginOutcome::Play(mc, layout) => {
                crate::dbg_log!("claim: logged in with protocol {}", proto);
                return run_play(mc, layout, cancel, emit).await;
            }
            LoginOutcome::Disconnected(reason) => {
                return Err(format!("Disconnected during login: {}", reason));
            }
            LoginOutcome::Rejected => {
                crate::dbg_log!("claim: protocol {} rejected, waiting out the connect throttle", proto);
                emit("Server closed the connection — waiting a few seconds before the next attempt...");
                // sleep ~6s in 500ms slices so cancel stays responsive
                for _ in 0..12 {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
            }
            LoginOutcome::Failed(e) => return Err(e),
        }
    }

    Err("The server closed the connection during login on every supported protocol version (1.19–1.20.1). It may require a newer Minecraft version.".into())
}

async fn attempt_login(
    cfg: &ClaimConfig,
    proto: i32,
    cancel: Arc<AtomicBool>,
    emit: &impl Fn(&str),
) -> LoginOutcome {
    // Packet layout is shared by 760/761, but the handshake protocol number
    // must be the exact version we're negotiating (a 760 login must declare
    // 760 on the wire, not 761).
    let layout = match Layout::for_protocol(proto) {
        Some(l) => l,
        None => return LoginOutcome::Failed(format!("Unsupported protocol {}", proto)),
    };
    let stream = match TcpStream::connect((cfg.server.as_str(), cfg.port)).await {
        Ok(s) => s,
        Err(e) => {
            return LoginOutcome::Failed(format!("Failed to connect to {}: {}", cfg.server, e))
        }
    };
    let mut mc = McStream::new(stream);

    let mut payload = Vec::new();
    payload.extend(varint_bytes(proto));
    payload.extend(varint_bytes(cfg.server.len() as i32));
    payload.extend_from_slice(cfg.server.as_bytes());
    payload.extend_from_slice(&cfg.port.to_be_bytes());
    payload.extend(varint_bytes(2)); // next state: login
    if let Err(e) = mc.write_packet(0x00, &payload).await {
        return LoginOutcome::Failed(e);
    }

    // login start
    // 759 (1.19):     Name + hasSigData(bool)
    // 760 (1.19.1/2): Name + hasSigData(bool) + hasUUID(bool) + UUID
    // 761+ (1.19.3+): Name + Optional<ProfileKey> + Optional<UUID>
    // We have no chat-signing key, so the profile key is always absent. With
    // the key absent, 760 and 761+ both serialize to the same bytes: a 0x00
    // marker followed by the present UUID. 760 must NOT get an extra
    // "Optional<ProfileKey> absent" byte (it has none), and 761+ must NOT get
    // a hasSigData byte (it was removed in 1.19.3) — either would desync the
    // packet and make the server drop the connection ("early EOF").
    let mut body = Vec::new();
    body.extend(varint_bytes(cfg.username.len() as i32));
    body.extend_from_slice(cfg.username.as_bytes());
    if proto == 759 {
        body.push(0x00);
    } else {
        body.push(0x00);
        body.push(0x01);
        body.extend_from_slice(&crypto::uuid_bytes(&cfg.uuid));
    }
    if let Err(e) = mc.write_packet(0x00, &body).await {
        return LoginOutcome::Failed(e);
    }

    // The first packet determines whether the proxy accepts this protocol
    // version at all. Some proxies close the connection right after Login
    // Start for versions they don't support — treat that as "try next".
    let first = match tokio::time::timeout(Duration::from_secs(6), mc.read_packet()).await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            crate::dbg_log!("claim: protocol {} closed right after login start: {}", proto, e);
            return LoginOutcome::Rejected;
        }
        Err(_) => {
            return LoginOutcome::Failed("No response to login start (connection stalled)".into())
        }
    };
    let mut id = first.0;
    let mut data = first.1;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return LoginOutcome::Failed("Cancelled".into());
        }
        match id {
            0x01 => {
                // encryption request
                let (server_id, mut i) = match parse_string(&data) {
                    Ok(v) => v,
                    Err(e) => return LoginOutcome::Failed(e),
                };
                let (pk_len, c) = match parse_varint(&data[i..]) {
                    Ok(v) => v,
                    Err(e) => return LoginOutcome::Failed(e),
                };
                i += c;
                let public_key = data[i..i + pk_len as usize].to_vec();
                i += pk_len as usize;
                let (vt_len, c) = match parse_varint(&data[i..]) {
                    Ok(v) => v,
                    Err(e) => return LoginOutcome::Failed(e),
                };
                i += c;
                let verify_token = data[i..i + vt_len as usize].to_vec();

                emit("Encrypting connection...");
                let secret = crypto::generate_shared_secret();
                let server_id_hash = crypto::compute_server_id(&server_id, &secret, &public_key);

                // mojang session join (retry once — Mojang occasionally drops it)
                let client = reqwest::Client::new();
                let uuid_undashed = crypto::uuid_without_dashes(&cfg.uuid);
                let mut join_ok = false;
                for attempt in 1..=2 {
                    if cancel.load(Ordering::Relaxed) {
                        return LoginOutcome::Failed("Cancelled".into());
                    }
                    let resp = match client
                        .post("https://sessionserver.mojang.com/session/minecraft/join")
                        .json(&serde_json::json!({
                            "accessToken": cfg.bearer_token,
                            "selectedProfile": uuid_undashed,
                            "serverId": server_id_hash,
                        }))
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => return LoginOutcome::Failed(format!("Session join failed: {}", e)),
                    };
                    if resp.status().is_success() {
                        join_ok = true;
                        break;
                    }
                    if attempt == 1 {
                        emit("Session server busy, retrying...");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
                if !join_ok {
                    let mut body = String::new();
                    if let Ok(resp) = client
                        .post("https://sessionserver.mojang.com/session/minecraft/join")
                        .json(&serde_json::json!({
                            "accessToken": cfg.bearer_token,
                            "selectedProfile": uuid_undashed,
                            "serverId": server_id_hash,
                        }))
                        .send()
                        .await
                    {
                        body = resp.text().await.unwrap_or_default();
                    }
                    let preview: String = body.chars().take(200).collect();
                    crate::dbg_log!("claim: session join failed: {}", preview);
                    return LoginOutcome::Failed(format!(
                        "Mojang rejected the session join (HTTP). Check that the signed-in account owns the skin/ign. {}",
                        preview
                    ));
                }
                crate::dbg_log!("claim: session join OK");

                let enc = match crypto::encrypt_login(&public_key, &secret, &verify_token) {
                    Ok(v) => v,
                    Err(e) => return LoginOutcome::Failed(e),
                };
                let mut ep = Vec::new();
                ep.extend(varint_bytes(enc.encrypted_secret.len() as i32));
                ep.extend(&enc.encrypted_secret);
                // 759-761 (1.19 - 1.19.2): a "has verify token" bool precedes the token,
                // because there's no chat-signing profile key to sign a salt instead.
                // 762+ (1.19.3+): back to plain sharedSecret + verifyToken.
                if proto < 762 {
                    ep.push(0x01);
                }
                ep.extend(varint_bytes(enc.encrypted_verify_token.len() as i32));
                ep.extend(&enc.encrypted_verify_token);
                if let Err(e) = mc.write_packet(0x01, &ep).await {
                    return LoginOutcome::Failed(e);
                }
                mc.enable_encryption(&secret);
            }
            0x03 => {
                // set compression
                let (threshold, _) = match parse_varint(&data) {
                    Ok(v) => v,
                    Err(e) => return LoginOutcome::Failed(e),
                };
                mc.set_compression(threshold);
            }
            0x02 => {
                // login success -> play
                return LoginOutcome::Play(mc, layout);
            }
            0x04 => {
                // login plugin request: acknowledge as unsuccessful
                let (msg_id, _) = match parse_varint(&data) {
                    Ok(v) => v,
                    Err(e) => return LoginOutcome::Failed(e),
                };
                let mut resp_body = Vec::new();
                resp_body.extend(varint_bytes(msg_id));
                resp_body.push(0x00);
                if let Err(e) = mc.write_packet(0x02, &resp_body).await {
                    return LoginOutcome::Failed(e);
                }
            }
            0x00 => {
                // login disconnect
                let (reason, _) =
                    parse_string(&data).unwrap_or_else(|_| ("unknown".to_string(), 0));
                return LoginOutcome::Disconnected(reason);
            }
            _ => {}
        }
        let (nid, ndata) = match mc.read_packet().await {
            Ok(v) => v,
            Err(e) => return LoginOutcome::Failed(e),
        };
        id = nid;
        data = ndata;
    }
}

async fn run_play(
    mut mc: McStream,
    layout: Layout,
    cancel: Arc<AtomicBool>,
    emit: impl Fn(&str),
) -> Result<String, String> {
    emit("Joined server, waiting...");
    let url_re = Regex::new(r#"https?://namemc\.com/[A-Za-z0-9?&=#._%+/:-]+"#).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let mut chat_sent_at: Option<Instant> = None;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        if start.elapsed() > Duration::from_secs(300) {
            return Err("Timed out (5 minutes)".into());
        }

        // send /namemc two seconds after entering play
        if chat_sent_at.is_none() && start.elapsed() >= Duration::from_millis(2000) {
            emit("Typing /namemc...");
            send_chat(&mut mc, layout, "/namemc").await?;
            chat_sent_at = Some(Instant::now());
        }

        // link timeout: 20s after the command was sent
        if let Some(sent) = &chat_sent_at {
            if sent.elapsed() > Duration::from_secs(20) {
                return Err("No NameMC link received from the server (timed out)".into());
            }
        }

        // Read with a generous timeout. This is deliberately much longer than
        // the biggest inter-packet gap (keepalives arrive every few seconds),
        // and firing it is treated as fatal: cancelling `read_packet` mid-read
        // would drop already-consumed bytes and desync the stream, so we must
        // never resume reading after a timeout.
        let pkt = tokio::time::timeout(Duration::from_secs(15), mc.read_packet()).await;
        let (id, data) = match pkt {
            Ok(Ok(v)) => v,
            Ok(Err(e)) if e.contains("early eof") => {
                return Err(
                    "Server closed the connection while waiting for the NameMC link (early EOF)."
                        .into(),
                )
            }
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("No data from server (connection stalled)".into()),
        };

        // keepalive response
        if id == layout.client_keepalive_id() && data.len() == 8 {
            mc.write_packet(layout.server_keepalive_id(), &data[..8]).await?;
            continue;
        }

        // kick
        if id == layout.client_kick_id() {
            if let Ok((reason, _)) = parse_string(&data) {
                return Err(format!("Disconnected by server: {}", reason));
            }
            return Err("Disconnected by server".into());
        }

        // scan every payload for the NameMC link (robust to chat packet id differences)
        let text = String::from_utf8_lossy(&data);
        if let Some(cap) = url_re.captures(&text) {
            let url = clean_url(cap.get(0).map(|m| m.as_str()).unwrap_or(""));
            if !url.is_empty() && url.contains("namemc.com") {
                return Ok(url);
            }
        }
    }
}
