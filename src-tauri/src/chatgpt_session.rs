//! ChatGPT OAuth 2.0 PKCE 認証 + セッション管理
//!
//! openai/codex CLI と同一パラメータ（公式実装ベース）:
//!   Auth:   https://auth.openai.com/oauth/authorize
//!   Token:  https://auth.openai.com/oauth/token
//!   Client: app_EMoamEEZ73f0CkXaXp7hrann
//!
//! フロー:
//!   1. PKCE verifier / challenge 生成
//!   2. ポート 1455 でローカル HTTP サーバー起動
//!   3. システムブラウザを開く（呼び出し元が担当）
//!   4. ブラウザが /auth/callback?code=…&state=… にリダイレクト
//!   5. code + verifier でトークン交換
//!   6. access_token を保存

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::{Emitter, Manager};

// ── OAuth 定数 ─────────────────────────────────
const AUTH_ENDPOINT: &str = "https://auth.openai.com/oauth/authorize";
const TOKEN_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const SCOPES: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const CALLBACK_PATH: &str = "/auth/callback";
const DEFAULT_PORT: u16 = 1455;

// ── ストレージ ────────────────────────────────
#[derive(Debug, Serialize, Deserialize, Default)]
struct SessionStore {
    access_token: Option<String>,
    refresh_token: Option<String>,
    vision_api_key: Option<String>,
}

fn session_path(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .expect("app_data_dir unavailable");
    std::fs::create_dir_all(&dir).ok();
    dir.join("chatgpt_session.json")
}

fn load_store(app: &tauri::AppHandle) -> SessionStore {
    let path = session_path(app);
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_store(app: &tauri::AppHandle, store: &SessionStore) {
    let path = session_path(app);
    if let Ok(json) = serde_json::to_string_pretty(store) {
        std::fs::write(path, json).ok();
    }
}

pub fn load_access_token(app: &tauri::AppHandle) -> Option<String> {
    load_store(app).access_token.filter(|s| !s.is_empty())
}

fn save_tokens(app: &tauri::AppHandle, access: String, refresh: Option<String>) {
    let mut store = load_store(app);
    store.access_token = Some(access);
    if let Some(r) = refresh {
        store.refresh_token = Some(r);
    }
    save_store(app, &store);
}

pub fn clear_access_token(app: &tauri::AppHandle) {
    let mut store = load_store(app);
    store.access_token = None;
    store.refresh_token = None;
    save_store(app, &store);
}

pub fn load_vision_api_key(app: &tauri::AppHandle) -> Option<String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            load_store(app)
                .vision_api_key
                .filter(|s| !s.is_empty())
        })
}

pub fn save_vision_api_key(app: &tauri::AppHandle, key: String) {
    let mut store = load_store(app);
    store.vision_api_key = if key.is_empty() { None } else { Some(key) };
    save_store(app, &store);
}

// ── PKCE ─────────────────────────────────────

pub fn generate_code_verifier() -> String {
    // 64バイトのランダム列（4×UUID = 64バイト）
    let mut bytes = [0u8; 64];
    for i in 0..4 {
        bytes[i * 16..(i + 1) * 16]
            .copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    }
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

pub fn generate_state() -> String {
    URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes())
}

// ── 認証 URL 構築 ─────────────────────────────

pub fn build_auth_url(challenge: &str, state: &str, port: u16) -> String {
    let redirect = format!("http://localhost:{port}{CALLBACK_PATH}");
    format!(
        "{AUTH_ENDPOINT}\
         ?response_type=code\
         &client_id={CLIENT_ID}\
         &redirect_uri={redirect}\
         &scope={scopes}\
         &code_challenge={challenge}\
         &code_challenge_method=S256\
         &state={state}\
         &id_token_add_organizations=true\
         &codex_cli_simplified_flow=true\
         &originator=math_teacher",
        redirect = percent_encode(&redirect),
        scopes = percent_encode(SCOPES),
        challenge = challenge,
        state = state,
    )
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

// ── ローカルコールバックサーバー ───────────────

struct CallbackResult {
    code: String,
    state: String,
}

/// ポートをバインドして成功したリスナーと使用ポートを返す
pub fn bind_callback_listener() -> Result<(std::net::TcpListener, u16), String> {
    let candidates = [DEFAULT_PORT, 8080, 8787, 9090, 0];
    for &port in &candidates {
        if let Ok(l) = std::net::TcpListener::bind(format!("127.0.0.1:{port}")) {
            let actual_port = l.local_addr().map_err(|e| e.to_string())?.port();
            return Ok((l, actual_port));
        }
    }
    Err("利用可能なポートが見つかりません".into())
}

/// ブラウザからのコールバックを1回待つ（ブロッキング）
pub fn wait_for_callback(
    listener: std::net::TcpListener,
) -> Result<CallbackResult, String> {
    use std::io::{Read, Write};

    for stream in listener.incoming() {
        let mut stream = stream.map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(300)))
            .ok();

        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
        let req = String::from_utf8_lossy(&buf[..n]);

        // GET /auth/callback?code=…&state=… を解析
        let first_line = req.lines().next().unwrap_or("");
        let path = first_line.split_whitespace().nth(1).unwrap_or("");

        if let Some(query) = path.split('?').nth(1) {
            let params = parse_query(query);
            if let (Some(code), Some(state)) = (params.get("code"), params.get("state")) {
                let html = "<html><body style='font-family:system-ui;text-align:center;padding:60px'>\
                    <h2>✅ ログイン完了！</h2><p>Math Teacher に戻ってください。このタブは閉じて構いません。</p>\
                    <script>setTimeout(()=>window.close(),2000)</script></body></html>";
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                        html.len(),
                        html
                    )
                    .as_bytes(),
                );
                return Ok(CallbackResult {
                    code: code.clone(),
                    state: state.clone(),
                });
            }
        }

        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
    }

    Err("コールバックサーバーが終了しました".into())
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next()?.to_string();
            let v = it.next().unwrap_or("").to_string();
            Some((k, v))
        })
        .collect()
}

// ── トークン交換 ──────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

async fn exchange_code(
    code: &str,
    verifier: &str,
    port: u16,
    client: &reqwest::Client,
) -> Result<TokenResponse, String> {
    let redirect = format!("http://localhost:{port}{CALLBACK_PATH}");
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &redirect),
        ("client_id", CLIENT_ID),
        ("code_verifier", verifier),
    ];

    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("トークン交換リクエスト失敗: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("トークン交換エラー {status}: {body}"));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| format!("トークンレスポンス解析失敗: {e}"))
}

// ── 公開: OAuthフロー完了処理（バックグラウンド実行）────

/// ブラウザを開いた後にバックグラウンドで呼ぶ。
/// コールバックを待ち、トークン交換を完了して保存する。
pub async fn complete_oauth_flow(
    app: tauri::AppHandle,
    listener: std::net::TcpListener,
    port: u16,
    verifier: String,
    expected_state: String,
) {
    // コールバックをブロッキングスレッドで待つ
    let result = tauri::async_runtime::spawn_blocking(move || wait_for_callback(listener))
        .await;

    let callback = match result {
        Ok(Ok(cb)) => cb,
        Ok(Err(e)) => {
            eprintln!("[OAuth] callback error: {e}");
            app.emit(
                "chatgpt-login-done",
                serde_json::json!({"ok": false, "error": e}),
            )
            .ok();
            return;
        }
        Err(e) => {
            let msg = e.to_string();
            eprintln!("[OAuth] join error: {msg}");
            app.emit(
                "chatgpt-login-done",
                serde_json::json!({"ok": false, "error": msg}),
            )
            .ok();
            return;
        }
    };

    if callback.state != expected_state {
        app.emit(
            "chatgpt-login-done",
            serde_json::json!({"ok": false, "error": "state mismatch"}),
        )
        .ok();
        return;
    }

    let client = reqwest::Client::new();
    match exchange_code(&callback.code, &verifier, port, &client).await {
        Ok(tokens) => {
            save_tokens(&app, tokens.access_token, tokens.refresh_token);
            app.emit("chatgpt-login-done", serde_json::json!({"ok": true}))
                .ok();
        }
        Err(e) => {
            eprintln!("[OAuth] token exchange error: {e}");
            app.emit(
                "chatgpt-login-done",
                serde_json::json!({"ok": false, "error": e}),
            )
            .ok();
        }
    }
}

// ── ChatGPT 内部API 呼び出し ──────────────────

/// ChatGPT 内部エンドポイントでテキスト生成
pub async fn chat_completion(
    client: &reqwest::Client,
    access_token: &str,
    system_prompt: &str,
    history: &[(String, String)],
    user_message: &str,
) -> Result<String, String> {
    let mut messages: Vec<serde_json::Value> = vec![];

    if !system_prompt.is_empty() {
        messages.push(serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "author": {"role": "system"},
            "content": {"content_type": "text", "parts": [system_prompt]},
            "metadata": {}
        }));
    }

    for (role, content) in history {
        messages.push(serde_json::json!({
            "id": uuid::Uuid::new_v4().to_string(),
            "author": {"role": role},
            "content": {"content_type": "text", "parts": [content]},
            "metadata": {}
        }));
    }

    let parent_id = messages
        .last()
        .and_then(|m| m["id"].as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    messages.push(serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "author": {"role": "user"},
        "content": {"content_type": "text", "parts": [user_message]},
        "metadata": {}
    }));

    let body = serde_json::json!({
        "action": "next",
        "messages": messages,
        "model": "gpt-4o",
        "timezone_offset_min": -540,
        "history_and_training_disabled": true,
        "conversation_id": null,
        "parent_message_id": parent_id
    });

    let resp = client
        .post("https://chatgpt.com/backend-api/conversation")
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/124.0.0.0 Safari/537.36",
        )
        .header("Origin", "https://chatgpt.com")
        .header("Referer", "https://chatgpt.com/")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("ChatGPT request failed: {e}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("ChatGPT セッション切れ。設定から再ログインしてください。".into());
    }
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        return Err(format!("ChatGPT API error {status}: {body_text}"));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| format!("ChatGPT read error: {e}"))?;

    parse_sse_last_content(&text).ok_or_else(|| "ChatGPT返答が空です".into())
}

fn parse_sse_last_content(sse: &str) -> Option<String> {
    let mut last = None;
    for line in sse.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data.trim() == "[DONE]" {
                break;
            }
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(parts) = val["message"]["content"]["parts"].as_array() {
                    if let Some(text) = parts.first().and_then(|p| p.as_str()) {
                        if !text.is_empty() {
                            last = Some(text.to_string());
                        }
                    }
                }
            }
        }
    }
    last
}
