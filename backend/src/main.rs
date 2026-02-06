use actix_cors::Cors;
use actix_session::{storage::CookieSessionStore, Session, SessionMiddleware};
use actix_web::cookie::{Key, SameSite};
use actix_web::http::header::LOCATION;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_ws::Message;
use futures_util::StreamExt;
use oauth2::basic::BasicClient;
use oauth2::reqwest::async_http_client;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
struct AppConfig {
    frontend_url: String,
    google_client_id: String,
    google_client_secret: String,
    google_redirect_url: String,
    session_secure: bool,
}

#[derive(Debug)]
struct AppState {
    users: Mutex<HashMap<String, UserProfile>>,
    online: Mutex<HashSet<String>>,
    messages: Mutex<HashMap<String, Vec<ChatMessage>>>,
    broadcaster: broadcast::Sender<ServerEvent>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UserProfile {
    id: String,
    display_name: String,
    avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeResponse {
    user: UserProfile,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PresenceEntry {
    id: String,
    display_name: String,
    avatar_url: Option<String>,
    status: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PresencePayload {
    users: Vec<PresenceEntry>,
    online_count: usize,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatMessage {
    id: String,
    from: String,
    to: String,
    text: String,
    timestamp: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", content = "data")]
enum ServerEvent {
    Presence(PresencePayload),
    Message(ChatMessage),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
enum ClientEvent {
    SendMessage { to: String, text: String },
}

#[derive(Debug, Deserialize)]
struct AuthCallbackQuery {
    code: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    sub: String,
    name: Option<String>,
    picture: Option<String>,
    email: Option<String>,
}

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().body("ok")
}

#[get("/api/me")]
async fn me(session: Session) -> impl Responder {
    match session.get::<UserProfile>("user") {
        Ok(Some(user)) => HttpResponse::Ok().json(MeResponse { user }),
        Ok(None) => HttpResponse::Unauthorized().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[get("/api/auth/google/login")]
async fn auth_google_login(config: web::Data<AppConfig>, session: Session) -> impl Responder {
    if config.google_client_id.is_empty() || config.google_client_secret.is_empty() {
        return HttpResponse::InternalServerError().body("Google OAuth is not configured");
    }

    let client = oauth_client(&config);
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    if session.insert("oauth_state", csrf_token.secret()).is_err() {
        return HttpResponse::InternalServerError().finish();
    }
    if session
        .insert("pkce_verifier", pkce_verifier.secret())
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Found()
        .append_header((LOCATION, auth_url.to_string()))
        .finish()
}

#[get("/api/auth/google/callback")]
async fn auth_google_callback(
    config: web::Data<AppConfig>,
    session: Session,
    state: web::Data<AppState>,
    query: web::Query<AuthCallbackQuery>,
) -> impl Responder {
    if config.google_client_id.is_empty() || config.google_client_secret.is_empty() {
        return HttpResponse::InternalServerError().body("Google OAuth is not configured");
    }
    let stored_state = match session.get::<String>("oauth_state") {
        Ok(Some(value)) => value,
        _ => return HttpResponse::BadRequest().body("Missing OAuth state"),
    };
    let pkce_verifier = match session.get::<String>("pkce_verifier") {
        Ok(Some(value)) => value,
        _ => return HttpResponse::BadRequest().body("Missing PKCE verifier"),
    };

    if stored_state != query.state {
        return HttpResponse::BadRequest().body("Invalid OAuth state");
    }

    let client = oauth_client(&config);
    let token_result = client
        .exchange_code(AuthorizationCode::new(query.code.clone()))
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
        .request_async(async_http_client)
        .await;

    let token = match token_result {
        Ok(token) => token,
        Err(_) => return HttpResponse::Unauthorized().body("Token exchange failed"),
    };

    let userinfo = match fetch_userinfo(token.access_token().secret()).await {
        Ok(info) => info,
        Err(_) => return HttpResponse::Unauthorized().body("User info fetch failed"),
    };

    let display_name = userinfo
        .name
        .or(userinfo.email)
        .unwrap_or_else(|| "HomeBound User".to_string());

    let user = UserProfile {
        id: userinfo.sub,
        display_name,
        avatar_url: userinfo.picture,
    };

    add_user(&state, &user);

    if session.insert("user", &user).is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    session.remove("oauth_state");
    session.remove("pkce_verifier");

    let redirect = format!("{}/dashboard", config.frontend_url.trim_end_matches('/'));
    HttpResponse::Found()
        .append_header((LOCATION, redirect))
        .finish()
}

#[post("/api/logout")]
async fn logout(session: Session) -> impl Responder {
    session.purge();
    HttpResponse::NoContent().finish()
}

#[get("/api/users")]
async fn list_users(session: Session, state: web::Data<AppState>) -> impl Responder {
    if session.get::<UserProfile>("user").ok().flatten().is_none() {
        return HttpResponse::Unauthorized().finish();
    }

    let payload = build_presence_payload(&state);
    HttpResponse::Ok().json(payload.users)
}

#[get("/api/messages/{user_id}")]
async fn get_messages(
    session: Session,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let current = match session.get::<UserProfile>("user") {
        Ok(Some(user)) => user,
        _ => return HttpResponse::Unauthorized().finish(),
    };

    let other_id = path.into_inner();
    let key = chat_key(&current.id, &other_id);
    let messages = state.messages.lock().unwrap();
    let history = messages.get(&key).cloned().unwrap_or_default();
    HttpResponse::Ok().json(history)
}

#[get("/api/ws")]
async fn ws(
    req: HttpRequest,
    stream: web::Payload,
    session: Session,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let current = match session.get::<UserProfile>("user") {
        Ok(Some(user)) => user,
        _ => return Ok(HttpResponse::Unauthorized().finish()),
    };

    add_user(&state, &current);
    set_online(&state, &current.id);

    let (response, mut ws_session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    let mut rx = state.broadcaster.subscribe();
    let user_id = current.id.clone();
    let state_clone = state.clone();

    actix_web::rt::spawn(async move {
        let _ = send_presence_direct(&state_clone, &mut ws_session).await;

        loop {
            tokio::select! {
                incoming = msg_stream.next() => {
                    match incoming {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(event) = serde_json::from_str::<ClientEvent>(&text) {
                                handle_client_event(&state_clone, &user_id, event);
                            }
                        }
                        Some(Ok(Message::Ping(bytes))) => {
                            let _ = ws_session.pong(&bytes).await;
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            break;
                        }
                        _ => {}
                    }
                }
                broadcast = rx.recv() => {
                    if let Ok(event) = broadcast {
                        if let Ok(payload) = serde_json::to_string(&event) {
                            let _ = ws_session.text(payload).await;
                        }
                    }
                }
            }
        }

        set_offline(&state_clone, &user_id);
        let _ = send_presence_broadcast(&state_clone);
        let _ = ws_session.close(None).await;
    });

    let _ = send_presence_broadcast(&state);
    Ok(response)
}

fn oauth_client(config: &AppConfig) -> BasicClient {
    BasicClient::new(
        ClientId::new(config.google_client_id.clone()),
        Some(ClientSecret::new(config.google_client_secret.clone())),
        AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
            .expect("Invalid auth URL"),
        Some(
            TokenUrl::new("https://oauth2.googleapis.com/token".to_string())
                .expect("Invalid token URL"),
        ),
    )
    .set_redirect_uri(
        RedirectUrl::new(config.google_redirect_url.clone()).expect("Invalid redirect URL"),
    )
}

async fn fetch_userinfo(access_token: &str) -> Result<GoogleUserInfo, reqwest::Error> {
    let client = reqwest::Client::new();
    client
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json::<GoogleUserInfo>()
        .await
}

fn build_session_key() -> Key {
    if let Ok(value) = std::env::var("SESSION_KEY") {
        let bytes = value.as_bytes();
        if bytes.len() >= 64 {
            return Key::from(bytes);
        }
        println!("SESSION_KEY must be at least 64 bytes; using a dev key");
    }

    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    Key::from(&bytes)
}

fn seed_users() -> HashMap<String, UserProfile> {
    HashMap::new()
}

fn add_user(state: &AppState, user: &UserProfile) {
    let mut user_map = state.users.lock().unwrap();
    user_map.entry(user.id.clone()).or_insert_with(|| user.clone());
}

fn set_online(state: &AppState, user_id: &str) {
    let mut online = state.online.lock().unwrap();
    online.insert(user_id.to_string());
}

fn set_offline(state: &AppState, user_id: &str) {
    let mut online = state.online.lock().unwrap();
    online.remove(user_id);
}

fn build_presence_payload(state: &AppState) -> PresencePayload {
    let user_map = state.users.lock().unwrap();
    let online = state.online.lock().unwrap();
    let mut entries = Vec::with_capacity(user_map.len());

    for user in user_map.values() {
        if !online.contains(&user.id) {
            continue;
        }

        entries.push(PresenceEntry {
            id: user.id.clone(),
            display_name: user.display_name.clone(),
            avatar_url: user.avatar_url.clone(),
            status: "online".to_string(),
        });
    }

    entries.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    let online_count = online.len();

    PresencePayload {
        users: entries,
        online_count,
    }
}

fn chat_key(a: &str, b: &str) -> String {
    if a < b {
        format!("{}:{}", a, b)
    } else {
        format!("{}:{}", b, a)
    }
}

fn store_message(state: &AppState, message: ChatMessage) {
    let key = chat_key(&message.from, &message.to);
    let mut messages_map = state.messages.lock().unwrap();
    let entry = messages_map.entry(key).or_insert_with(Vec::new);
    entry.push(message);
}

fn handle_client_event(state: &AppState, user_id: &str, event: ClientEvent) {
    match event {
        ClientEvent::SendMessage { to, text } => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return;
            }

            let message = ChatMessage {
                id: format!("m{}", rand::random::<u64>()),
                from: user_id.to_string(),
                to: to.clone(),
                text: trimmed.to_string(),
                timestamp: OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| "".to_string()),
            };

            store_message(state, message.clone());
            let _ = state.broadcaster.send(ServerEvent::Message(message));
        }
    }
}

async fn send_presence_direct(
    state: &AppState,
    ws_session: &mut actix_ws::Session,
) -> Result<(), actix_ws::Closed> {
    let payload = ServerEvent::Presence(build_presence_payload(state));
    let body = serde_json::to_string(&payload).unwrap_or_default();
    ws_session.text(body).await
}

fn send_presence_broadcast(state: &AppState) -> Result<usize, broadcast::error::SendError<ServerEvent>> {
    let payload = ServerEvent::Presence(build_presence_payload(state));
    state.broadcaster.send(payload)
}

fn load_config() -> AppConfig {
    let frontend_url = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
    let google_client_id = std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default();
    let google_client_secret = std::env::var("GOOGLE_CLIENT_SECRET").unwrap_or_default();
    let google_redirect_url = std::env::var("GOOGLE_REDIRECT_URL")
        .unwrap_or_else(|_| "http://localhost:8080/api/auth/google/callback".to_string());
    let session_secure = std::env::var("SESSION_SECURE")
        .ok()
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(false);

    AppConfig {
        frontend_url,
        google_client_id,
        google_client_secret,
        google_redirect_url,
        session_secure,
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = dotenvy::dotenv();
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let config = load_config();
    let session_key = build_session_key();
    let (broadcaster, _) = broadcast::channel(200);
    let state = web::Data::new(AppState {
        users: Mutex::new(seed_users()),
        online: Mutex::new(HashSet::new()),
        messages: Mutex::new(HashMap::new()),
        broadcaster,
    });

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin(&config.frontend_url)
            .allowed_methods(vec!["GET", "POST", "OPTIONS"])
            .allow_any_header()
            .supports_credentials();

        App::new()
            .app_data(web::Data::new(config.clone()))
            .app_data(state.clone())
            .wrap(cors)
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), session_key.clone())
                    .cookie_name("hb3.session".to_string())
                    .cookie_http_only(true)
                    .cookie_same_site(SameSite::Lax)
                    .cookie_secure(config.session_secure)
                    .build(),
            )
            .service(health)
            .service(me)
            .service(auth_google_login)
            .service(auth_google_callback)
            .service(logout)
            .service(list_users)
            .service(get_messages)
            .service(ws)
    })
    .bind(bind_addr)?
    .run()
    .await
}
