use actix_cors::Cors;
use actix_files::NamedFile;
use actix_multipart::Multipart;
use actix_session::{storage::CookieSessionStore, Session, SessionMiddleware};
use actix_web::cookie::{Key, SameSite};
use actix_web::http::header::LOCATION;
use actix_web::{delete, get, post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
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
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::fs as std_fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio::{fs, io::AsyncWriteExt, process::Command, time::sleep};
use uuid::Uuid;

#[derive(Debug, Clone)]
struct AppConfig {
    frontend_url: String,
    google_client_id: String,
    google_client_secret: String,
    google_redirect_url: String,
    session_secure: bool,
    media_originals_path: String,
    media_hls_path: String,
    media_thumbnails_path: String,
    media_subtitles_path: String,
    upload_tmp_path: String,
    max_upload_bytes: u64,
    allowed_video_formats: Vec<String>,
    ffmpeg_path: Option<String>,
}

#[derive(Debug)]
struct AppState {
    users: Mutex<HashMap<String, UserProfile>>,
    online: Mutex<HashSet<String>>,
    messages: Mutex<HashMap<String, Vec<ChatMessage>>>,
    broadcaster: broadcast::Sender<ServerEvent>,
    db: PgPool,
    stream_channels: Mutex<HashMap<String, broadcast::Sender<StreamEvent>>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct UserProfile {
    id: String,
    display_name: String,
    avatar_url: Option<String>,
    email: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadMeta {
    movie_id: Uuid,
    original_filename: String,
    ext: String,
    uploader_id: Option<Uuid>,
    movie_title: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UploadResponse {
    movie_id: Uuid,
    upload_id: String,
    filename: String,
    chunk_index: usize,
    total_chunks: usize,
    bytes_received: u64,
    status: String,
}

#[derive(Debug, sqlx::FromRow)]
struct PendingMovie {
    id: Uuid,
    filename: String,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
struct MovieRow {
    id: Uuid,
    filename: String,
    movie_title: Option<String>,
    description: Option<String>,
    subtitle_filename: Option<String>,
    hls_path: Option<String>,
    transcoding_status: String,
    duration_seconds: Option<i32>,
    file_size_bytes: Option<i64>,
    upload_date: OffsetDateTime,
    uploader_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct MovieListQuery {
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, sqlx::FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamRow {
    stream_id: String,
    user_id: Uuid,
    status: String,
    created_at: OffsetDateTime,
    ended_at: Option<OffsetDateTime>,
    current_movie_id: Option<Uuid>,
    current_timestamp: f64,
    is_playing: bool,
    stream_type: String,
    viewer_count: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamCreateResponse {
    stream_id: String,
    stream_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamStartRequest {
    movie_id: Uuid,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct StreamViewerRow {
    id: Uuid,
    user_id: Uuid,
    joined_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CameraStartResponse {
    ws_url: String,
    hls_url: String,
}

#[derive(Debug, sqlx::FromRow)]
struct StreamViewerInfoRow {
    user_id: Uuid,
    email: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", content = "data")]
enum ServerEvent {
    Presence(PresencePayload),
    Message(ChatMessage),
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Viewer {
    id: Uuid,
    display_name: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    PlaybackSync { action: String, timestamp: f64, host_id: String },
    MovieStart { movie_id: String },
    StreamEnded { reason: String },
    ChatMessage {
        user_id: String,
        username: String,
        message: String,
        timestamp: String,
    },
    ViewerUpdate { count: i32, viewers: Vec<Viewer> },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamClientEvent {
    PlaybackControl { action: String, timestamp: f64 },
    MovieStart { movie_id: String },
    StreamEnd,
    ChatMessage { message: String },
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

#[derive(Debug, sqlx::FromRow)]
struct DbUserId {
    id: Uuid,
}

fn header_as_string(req: &HttpRequest, name: &str) -> Option<String> {
    req.headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn read_text_field(mut field: actix_multipart::Field) -> Result<String, actix_web::Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = field.next().await {
        let data = chunk?;
        bytes.extend_from_slice(&data);
    }
    Ok(String::from_utf8_lossy(&bytes).trim().to_string())
}

fn infer_extension(bytes: &[u8]) -> Option<String> {
    infer::get(bytes).map(|kind| kind.extension().to_lowercase())
}

#[post("/api/movies/upload")]
async fn upload_movie(
    req: HttpRequest,
    mut payload: Multipart,
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
) -> impl Responder {
    let upload_id = header_as_string(&req, "x-upload-id").unwrap_or_else(|| Uuid::new_v4().to_string());
    let chunk_index = header_as_string(&req, "x-chunk-index")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let total_chunks = header_as_string(&req, "x-total-chunks")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1);

    if chunk_index >= total_chunks {
        return HttpResponse::BadRequest().body("Invalid chunk index");
    }

    let tmp_dir = Path::new(&config.upload_tmp_path);
    let part_path = tmp_dir.join(format!("{}.part", upload_id));
    let meta_path = tmp_dir.join(format!("{}.json", upload_id));

    let mut meta: Option<UploadMeta> = None;
    if let Ok(bytes) = fs::read(&meta_path).await {
        if let Ok(existing) = serde_json::from_slice::<UploadMeta>(&bytes) {
            meta = Some(existing);
        }
    }

    if chunk_index > 0 && meta.is_none() {
        return HttpResponse::BadRequest().body("Missing upload metadata");
    }

    let mut movie_title: Option<String> = None;
    let mut description: Option<String> = None;
    let mut uploader_id: Option<Uuid> = None;
    let mut original_filename: Option<String> = None;
    let mut ext_override: Option<String> = None;
    let mut wrote_file = false;
    let mut bytes_received: u64 = 0;

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(field) => field,
            Err(_) => return HttpResponse::BadRequest().body("Invalid multipart payload"),
        };

        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "file" {
            let existing_len = if chunk_index == 0 {
                0
            } else {
                fs::metadata(&part_path)
                    .await
                    .map(|meta| meta.len())
                    .unwrap_or(0)
            };

            let mut file = match fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(chunk_index > 0)
                .truncate(chunk_index == 0)
                .open(&part_path)
                .await
            {
                Ok(file) => file,
                Err(_) => return HttpResponse::InternalServerError().body("Failed to open upload file"),
            };

            let mut header_bytes = Vec::new();
            let filename = field
                .content_disposition()
                .and_then(|content| content.get_filename())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "upload".to_string());
            original_filename = Some(filename);

            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(_) => return HttpResponse::BadRequest().body("Failed to read upload chunk"),
                };

                if chunk_index == 0 && header_bytes.len() < 512 {
                    let remaining = 512 - header_bytes.len();
                    let take_len = remaining.min(data.len());
                    header_bytes.extend_from_slice(&data[..take_len]);
                }

                bytes_received = bytes_received.saturating_add(data.len() as u64);
                let total_written = existing_len.saturating_add(bytes_received);
                if total_written > config.max_upload_bytes {
                    let _ = fs::remove_file(&part_path).await;
                    return HttpResponse::PayloadTooLarge().body("File exceeds max upload size");
                }

                if let Err(_) = file.write_all(&data).await {
                    return HttpResponse::InternalServerError().body("Failed to write upload");
                }
            }

            if chunk_index == 0 {
                ext_override = infer_extension(&header_bytes);
            }
            wrote_file = true;
        } else if field_name == "movie_title" {
            if let Ok(value) = read_text_field(field).await {
                if !value.is_empty() {
                    movie_title = Some(value);
                }
            }
        } else if field_name == "description" {
            if let Ok(value) = read_text_field(field).await {
                if !value.is_empty() {
                    description = Some(value);
                }
            }
        } else if field_name == "uploader_id" {
            if let Ok(value) = read_text_field(field).await {
                uploader_id = Uuid::parse_str(&value).ok();
            }
        }
    }

    if !wrote_file {
        return HttpResponse::BadRequest().body("Missing file field");
    }

    if chunk_index == 0 {
        let ext = match ext_override {
            Some(ext) => ext,
            None => {
                let _ = fs::remove_file(&part_path).await;
                return HttpResponse::BadRequest().body("Unsupported file type");
            }
        };

        if !config.allowed_video_formats.contains(&ext) {
            let _ = fs::remove_file(&part_path).await;
            return HttpResponse::BadRequest().body("File type not allowed");
        }

        let meta_value = UploadMeta {
            movie_id: Uuid::new_v4(),
            original_filename: original_filename.unwrap_or_else(|| "upload".to_string()),
            ext,
            uploader_id,
            movie_title,
            description,
        };

        if let Ok(payload) = serde_json::to_vec(&meta_value) {
            if fs::write(&meta_path, payload).await.is_err() {
                return HttpResponse::InternalServerError().body("Failed to write upload metadata");
            }
        }

        meta = Some(meta_value);
    }

    let meta = match meta {
        Some(meta) => meta,
        None => return HttpResponse::InternalServerError().body("Upload metadata missing"),
    };

    if chunk_index + 1 < total_chunks {
        return HttpResponse::Ok().json(UploadResponse {
            movie_id: meta.movie_id,
            upload_id,
            filename: meta.original_filename,
            chunk_index,
            total_chunks,
            bytes_received,
            status: "partial".to_string(),
        });
    }

    let stored_filename = format!("{}.{}", meta.movie_id, meta.ext);
    let final_path = Path::new(&config.media_originals_path).join(&stored_filename);

    if fs::rename(&part_path, &final_path).await.is_err() {
        return HttpResponse::InternalServerError().body("Failed to finalize upload");
    }

    let file_size = fs::metadata(&final_path)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);

    let file_size_i64 = i64::try_from(file_size).unwrap_or(i64::MAX);
    let insert_result = sqlx::query(
        "INSERT INTO movies (id, filename, movie_title, description, subtitle_filename, hls_path, transcoding_status, duration_seconds, file_size_bytes, upload_date, uploader_id) \
         VALUES ($1, $2, $3, $4, NULL, NULL, 'pending'::transcoding_status, NULL, $5, now(), $6)",
    )
    .bind(meta.movie_id)
    .bind(&stored_filename)
    .bind(meta.movie_title)
    .bind(meta.description)
    .bind(file_size_i64)
    .bind(meta.uploader_id)
    .execute(&state.db)
    .await;

    if insert_result.is_err() {
        return HttpResponse::InternalServerError().body("Failed to record upload");
    }

    let _ = fs::remove_file(&meta_path).await;

    HttpResponse::Ok().json(UploadResponse {
        movie_id: meta.movie_id,
        upload_id,
        filename: stored_filename,
        chunk_index,
        total_chunks,
        bytes_received: file_size,
        status: "completed".to_string(),
    })
}

#[get("/api/movies")]
async fn list_movies(
    state: web::Data<AppState>,
    query: web::Query<MovieListQuery>,
) -> impl Responder {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);
    let offset = (page - 1) as i64 * page_size as i64;

    let rows = sqlx::query_as::<_, MovieRow>(
        "SELECT id, filename, movie_title, description, subtitle_filename, hls_path, \
            transcoding_status::text as transcoding_status, duration_seconds, file_size_bytes, \
            upload_date, uploader_id \
         FROM movies ORDER BY upload_date DESC LIMIT $1 OFFSET $2",
    )
    .bind(page_size as i64)
    .bind(offset)
    .fetch_all(&state.db)
    .await;

    match rows {
        Ok(rows) => HttpResponse::Ok().json(rows),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[get("/api/movies/{id}")]
async fn get_movie(state: web::Data<AppState>, path: web::Path<Uuid>) -> impl Responder {
    let movie_id = path.into_inner();
    let row = sqlx::query_as::<_, MovieRow>(
        "SELECT id, filename, movie_title, description, subtitle_filename, hls_path, \
            transcoding_status::text as transcoding_status, duration_seconds, file_size_bytes, \
            upload_date, uploader_id \
         FROM movies WHERE id = $1",
    )
    .bind(movie_id)
    .fetch_optional(&state.db)
    .await;

    match row {
        Ok(Some(movie)) => HttpResponse::Ok().json(movie),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[put("/api/movies/{id}")]
async fn update_movie(
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
    path: web::Path<Uuid>,
    mut payload: Multipart,
) -> impl Responder {
    let movie_id = path.into_inner();
    let mut movie_title: Option<String> = None;
    let mut description: Option<String> = None;
    let mut thumbnail_bytes: Option<Vec<u8>> = None;
    let mut thumbnail_written = false;

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(field) => field,
            Err(_) => return HttpResponse::BadRequest().body("Invalid multipart payload"),
        };

        let field_name = field.name().unwrap_or("").to_string();
        if field_name == "movie_title" {
            if let Ok(value) = read_text_field(field).await {
                if !value.is_empty() {
                    movie_title = Some(value);
                }
            }
        } else if field_name == "description" {
            if let Ok(value) = read_text_field(field).await {
                if !value.is_empty() {
                    description = Some(value);
                }
            }
        } else if field_name == "thumbnail" {
            let mut bytes = Vec::new();
            while let Some(chunk) = field.next().await {
                let data = match chunk {
                    Ok(data) => data,
                    Err(_) => return HttpResponse::BadRequest().body("Failed to read thumbnail"),
                };
                if bytes.len() + data.len() > 5 * 1024 * 1024 {
                    return HttpResponse::PayloadTooLarge().body("Thumbnail too large");
                }
                bytes.extend_from_slice(&data);
            }

            if !bytes.is_empty() {
                let thumbnail_path = Path::new(&config.media_thumbnails_path)
                    .join(format!("{}.jpg", movie_id));
                if fs::write(&thumbnail_path, &bytes).await.is_err() {
                    return HttpResponse::InternalServerError().body("Failed to save thumbnail");
                }
                thumbnail_written = true;
                thumbnail_bytes = Some(bytes);
            }
        }
    }

    let result = sqlx::query(
        "UPDATE movies SET movie_title = COALESCE($2, movie_title), \
            description = COALESCE($3, description), \
            movie_image = COALESCE($4, movie_image), \
            updated_at = now() \
         WHERE id = $1",
    )
    .bind(movie_id)
    .bind(movie_title)
    .bind(description)
    .bind(thumbnail_bytes)
    .execute(&state.db)
    .await;

    match result {
        Ok(result) if result.rows_affected() > 0 => {
            if thumbnail_written {
                HttpResponse::Ok().body("Updated")
            } else {
                HttpResponse::Ok().body("Updated")
            }
        }
        Ok(_) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[delete("/api/movies/{id}")]
async fn delete_movie(
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
    path: web::Path<Uuid>,
) -> impl Responder {
    let movie_id = path.into_inner();
    let row = sqlx::query_as::<_, PendingMovie>(
        "SELECT id, filename FROM movies WHERE id = $1",
    )
    .bind(movie_id)
    .fetch_optional(&state.db)
    .await;

    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => return HttpResponse::NotFound().finish(),
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let original_path = Path::new(&config.media_originals_path).join(&row.filename);
    let _ = fs::remove_file(&original_path).await;

    let hls_dir = Path::new(&config.media_hls_path).join(row.id.to_string());
    let _ = fs::remove_dir_all(&hls_dir).await;

    let thumbnail_path = Path::new(&config.media_thumbnails_path)
        .join(format!("{}.jpg", row.id));
    let _ = fs::remove_file(&thumbnail_path).await;

    let delete_result = sqlx::query("DELETE FROM movies WHERE id = $1")
        .bind(row.id)
        .execute(&state.db)
        .await;

    match delete_result {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[get("/hls/{movie_id}/master.m3u8")]
async fn hls_master(
    config: web::Data<AppConfig>,
    path: web::Path<String>,
) -> Result<NamedFile, actix_web::Error> {
    let movie_id = path.into_inner();
    let root = hls_root_for(&config, &movie_id)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid movie id"))?;
    let file_path = root.join("master.m3u8");
    let file = NamedFile::open_async(file_path).await?;
    Ok(file.set_content_type(m3u8_mime()))
}

#[get("/hls/{movie_id}/v{variant}/playlist.m3u8")]
async fn hls_variant_playlist(
    config: web::Data<AppConfig>,
    path: web::Path<(String, u8)>,
) -> Result<NamedFile, actix_web::Error> {
    let (movie_id, variant) = path.into_inner();
    let root = hls_root_for(&config, &movie_id)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid movie id"))?;
    let file_path = root.join(format!("v{}", variant)).join("playlist.m3u8");
    let file = NamedFile::open_async(file_path).await?;
    Ok(file.set_content_type(m3u8_mime()))
}

#[get("/hls/{movie_id}/v{variant}/segment{segment}.ts")]
async fn hls_segment(
    config: web::Data<AppConfig>,
    path: web::Path<(String, u8, u32)>,
) -> Result<NamedFile, actix_web::Error> {
    let (movie_id, variant, segment) = path.into_inner();
    let root = hls_root_for(&config, &movie_id)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid movie id"))?;
    let filename = format!("segment{:03}.ts", segment);
    let file_path = root.join(format!("v{}", variant)).join(filename);
    let file = NamedFile::open_async(file_path).await?;
    Ok(file.set_content_type(ts_mime()))
}

#[post("/api/streams/create")]
async fn create_stream(
    session: Session,
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
) -> impl Responder {
    let user = match session.get::<UserProfile>("user") {
        Ok(Some(user)) => user,
        _ => return HttpResponse::Unauthorized().finish(),
    };
    let user_id = match Uuid::parse_str(&user.id) {
        Ok(value) => value,
        Err(_) => return HttpResponse::BadRequest().body("Invalid user id"),
    };

    let stream_id = build_stream_id(&user);

    let insert_result = sqlx::query(
        "INSERT INTO streams (stream_id, user_id, status, created_at, current_movie_id) \
         VALUES ($1, $2, 'starting', now(), NULL) \
         ON CONFLICT (stream_id) DO UPDATE SET status = 'starting', current_movie_id = NULL",
    )
    .bind(&stream_id)
    .bind(user_id)
    .execute(&state.db)
    .await;

    if insert_result.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    let stream_url = format!("{}/stream/{}", config.frontend_url.trim_end_matches('/'), stream_id);
    HttpResponse::Ok().json(StreamCreateResponse { stream_id, stream_url })
}

#[get("/api/streams")]
async fn list_streams(state: web::Data<AppState>) -> impl Responder {
    let rows = sqlx::query_as::<_, StreamRow>(
        "SELECT stream_id, user_id, status::text as status, created_at, ended_at, current_movie_id, \
            \"current_timestamp\" as current_timestamp, is_playing, stream_type::text as stream_type, viewer_count \
         FROM streams WHERE status = 'live' ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await;

    match rows {
        Ok(rows) => HttpResponse::Ok().json(rows),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[get("/api/streams/{stream_id}")]
async fn get_stream(state: web::Data<AppState>, path: web::Path<String>) -> impl Responder {
    let stream_id = path.into_inner();
    let row = sqlx::query_as::<_, StreamRow>(
        "SELECT stream_id, user_id, status::text as status, created_at, ended_at, current_movie_id, \
            \"current_timestamp\" as current_timestamp, is_playing, stream_type::text as stream_type, viewer_count \
         FROM streams WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .fetch_optional(&state.db)
    .await;

    match row {
        Ok(Some(stream)) => HttpResponse::Ok().json(stream),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[post("/api/streams/{stream_id}/start")]
async fn start_stream(
    session: Session,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<StreamStartRequest>,
) -> impl Responder {
    let user_id = match current_user_id(&session) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let stream_id = path.into_inner();

    let owner_check = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM streams WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .fetch_optional(&state.db)
    .await;

    let owner_id = match owner_check {
        Ok(Some(value)) => value,
        Ok(None) => return HttpResponse::NotFound().finish(),
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    if owner_id != user_id {
        return HttpResponse::Forbidden().finish();
    }

    let result = sqlx::query(
        "UPDATE streams SET status = 'live', current_movie_id = $2, \"current_timestamp\" = 0, \
            is_playing = true, stream_type = 'movie' \
         WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .bind(payload.movie_id)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[post("/api/streams/{stream_id}/end")]
async fn end_stream(
    session: Session,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let user_id = match current_user_id(&session) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let stream_id = path.into_inner();

    let owner_check = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM streams WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .fetch_optional(&state.db)
    .await;

    let owner_id = match owner_check {
        Ok(Some(value)) => value,
        Ok(None) => return HttpResponse::NotFound().finish(),
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    if owner_id != user_id {
        return HttpResponse::Forbidden().finish();
    }

    let result = sqlx::query(
        "UPDATE streams SET status = 'ended', ended_at = now(), is_playing = false WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[get("/api/streams/{stream_id}/viewers")]
async fn list_stream_viewers(
    session: Session,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    if session.get::<UserProfile>("user").ok().flatten().is_none() {
        return HttpResponse::Unauthorized().finish();
    }

    let stream_id = path.into_inner();
    let rows = sqlx::query_as::<_, StreamViewerRow>(
        "SELECT id, user_id, joined_at FROM stream_viewers WHERE stream_id = $1 AND is_active = true ORDER BY joined_at ASC",
    )
    .bind(&stream_id)
    .fetch_all(&state.db)
    .await;

    match rows {
        Ok(rows) => HttpResponse::Ok().json(rows),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[post("/api/streams/{stream_id}/camera/start")]
async fn start_camera_stream(
    session: Session,
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
    path: web::Path<String>,
) -> impl Responder {
    let user_id = match current_user_id(&session) {
        Ok(value) => value,
        Err(resp) => return resp,
    };
    let stream_id = path.into_inner();

    let owner_check = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM streams WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .fetch_optional(&state.db)
    .await;

    let owner_id = match owner_check {
        Ok(Some(value)) => value,
        Ok(None) => return HttpResponse::NotFound().finish(),
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    if owner_id != user_id {
        return HttpResponse::Forbidden().finish();
    }

    let output_dir = match camera_hls_root_for(&config, &stream_id) {
        Some(path) => path,
        None => return HttpResponse::BadRequest().body("Invalid stream id"),
    };

    if fs::create_dir_all(&output_dir).await.is_err() {
        return HttpResponse::InternalServerError().body("Failed to create HLS directory");
    }

    let update_result = sqlx::query(
        "UPDATE streams SET stream_type = 'camera' WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .execute(&state.db)
    .await;

    if update_result.is_err() {
        return HttpResponse::InternalServerError().finish();
    }

    let ws_url = format!("/ws/camera/{}", stream_id);
    let hls_url = format!("/hls/camera-{}/live.m3u8", stream_id);
    HttpResponse::Ok().json(CameraStartResponse { ws_url, hls_url })
}

#[get("/ws/camera/{stream_id}")]
async fn ws_camera(
    req: HttpRequest,
    stream: web::Payload,
    session: Session,
    state: web::Data<AppState>,
    config: web::Data<AppConfig>,
    path: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = match current_user_id(&session) {
        Ok(value) => value,
        Err(resp) => return Ok(resp),
    };
    let stream_id = path.into_inner();

    let owner_check = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM streams WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .fetch_optional(&state.db)
    .await;

    let owner_id = match owner_check {
        Ok(Some(value)) => value,
        Ok(None) => return Ok(HttpResponse::NotFound().finish()),
        Err(_) => return Ok(HttpResponse::InternalServerError().finish()),
    };

    if owner_id != user_id {
        return Ok(HttpResponse::Forbidden().finish());
    }

    let output_dir = match camera_hls_root_for(&config, &stream_id) {
        Some(path) => path,
        None => return Ok(HttpResponse::BadRequest().body("Invalid stream id")),
    };

    if fs::create_dir_all(&output_dir).await.is_err() {
        return Ok(HttpResponse::InternalServerError().body("Failed to create HLS directory"));
    }

    let ffmpeg = resolve_ffmpeg_path(&config);
    let playlist_path = output_dir.join("live.m3u8");
    let log_path = output_dir.join("camera.log");

    let mut child = Command::new(ffmpeg)
        .arg("-f")
        .arg("webm")
        .arg("-i")
        .arg("pipe:0")
        .arg("-codec:v")
        .arg("libx264")
        .arg("-preset")
        .arg("ultrafast")
        .arg("-tune")
        .arg("zerolatency")
        .arg("-codec:a")
        .arg("aac")
        .arg("-b:a")
        .arg("128k")
        .arg("-f")
        .arg("hls")
        .arg("-hls_time")
        .arg("2")
        .arg("-hls_list_size")
        .arg("5")
        .arg("-hls_flags")
        .arg("delete_segments+append_list")
        .arg(playlist_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(std_fs::File::create(log_path).unwrap_or_else(|_| std_fs::File::create("camera.log").unwrap())))
        .spawn()
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let mut stdin = child.stdin.take();
    let (response, mut ws_session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    actix_web::rt::spawn(async move {
        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    let _ = ws_session.ping(b"hb").await;
                }
                incoming = msg_stream.next() => {
                    match incoming {
                        Some(Ok(Message::Binary(bytes))) => {
                            if let Some(stdin) = stdin.as_mut() {
                                if stdin.write_all(&bytes).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            break;
                        }
                        Some(Ok(Message::Ping(bytes))) => {
                            let _ = ws_session.pong(&bytes).await;
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(mut stdin) = stdin {
            let _ = stdin.shutdown().await;
        }

        let _ = child.kill().await;
        let _ = ws_session.close(None).await;
    });

    Ok(response)
}

#[get("/hls/camera-{stream_id}/live.m3u8")]
async fn camera_hls_master(
    config: web::Data<AppConfig>,
    path: web::Path<String>,
) -> Result<NamedFile, actix_web::Error> {
    let stream_id = path.into_inner();
    let root = camera_hls_root_for(&config, &stream_id)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid stream id"))?;
    let file_path = root.join("live.m3u8");
    let file = NamedFile::open_async(file_path).await?;
    Ok(file.set_content_type(m3u8_mime()))
}

#[get("/hls/camera-{stream_id}/segment{segment}.ts")]
async fn camera_hls_segment(
    config: web::Data<AppConfig>,
    path: web::Path<(String, u32)>,
) -> Result<NamedFile, actix_web::Error> {
    let (stream_id, segment) = path.into_inner();
    let root = camera_hls_root_for(&config, &stream_id)
        .ok_or_else(|| actix_web::error::ErrorBadRequest("Invalid stream id"))?;
    let filename = format!("segment{:03}.ts", segment);
    let file_path = root.join(filename);
    let file = NamedFile::open_async(file_path).await?;
    Ok(file.set_content_type(ts_mime()))
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
        .or(userinfo.email.clone())
        .unwrap_or_else(|| "HomeBound User".to_string());

    let email = userinfo
        .email
        .clone()
        .unwrap_or_else(|| format!("user-{}@local", userinfo.sub));

    let db_user = sqlx::query_as::<_, DbUserId>(
        "INSERT INTO users (email) VALUES ($1) \
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email \
         RETURNING id",
    )
    .bind(&email)
    .fetch_one(&state.db)
    .await;

    let db_user = match db_user {
        Ok(user) => user,
        Err(_) => return HttpResponse::InternalServerError().body("Failed to persist user"),
    };

    let user = UserProfile {
        id: db_user.id.to_string(),
        display_name,
        avatar_url: userinfo.picture,
        email: Some(email),
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

#[get("/ws/stream/{stream_id}")]
async fn ws_stream(
    req: HttpRequest,
    stream: web::Payload,
    session: Session,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    let current = match session.get::<UserProfile>("user") {
        Ok(Some(user)) => user,
        _ => return Ok(HttpResponse::Unauthorized().finish()),
    };

    let user_id = match Uuid::parse_str(&current.id) {
        Ok(value) => value,
        Err(_) => return Ok(HttpResponse::BadRequest().body("Invalid user id")),
    };

    let stream_id = path.into_inner();
    let viewer_id = Uuid::new_v4();

    let insert_result = sqlx::query(
        "INSERT INTO stream_viewers (id, stream_id, user_id, joined_at, is_active) \
         VALUES ($1, $2, $3, now(), true)",
    )
    .bind(viewer_id)
    .bind(&stream_id)
    .bind(user_id)
    .execute(&state.db)
    .await;

    if insert_result.is_err() {
        return Ok(HttpResponse::InternalServerError().finish());
    }

    let _ = sqlx::query(
        "UPDATE streams SET viewer_count = viewer_count + 1 WHERE stream_id = $1",
    )
    .bind(&stream_id)
    .execute(&state.db)
    .await;

    let channel = stream_channel(&state, &stream_id);
    let mut rx = channel.subscribe();

    let (response, mut ws_session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    let state_clone = state.clone();
    let stream_id_clone = stream_id.clone();
    let current_clone = current.clone();
    let channel_clone = channel.clone();

    actix_web::rt::spawn(async move {
        if let Some(update) = build_viewer_update(&state_clone, &stream_id_clone).await {
            let _ = channel_clone.send(update);
        }

        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    let _ = ws_session.ping(b"hb").await;
                }
                incoming = msg_stream.next() => {
                    match incoming {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(event) = serde_json::from_str::<StreamClientEvent>(&text) {
                                match event {
                                    StreamClientEvent::PlaybackControl { action, timestamp } => {
                                        let payload = StreamEvent::PlaybackSync {
                                            action,
                                            timestamp,
                                            host_id: current_clone.id.clone(),
                                        };
                                        let _ = channel_clone.send(payload);
                                    }
                                    StreamClientEvent::MovieStart { movie_id } => {
                                        let _ = channel_clone.send(StreamEvent::MovieStart { movie_id });
                                    }
                                    StreamClientEvent::StreamEnd => {
                                        let _ = sqlx::query(
                                            "UPDATE streams SET status = 'ended', ended_at = now(), is_playing = false WHERE stream_id = $1",
                                        )
                                        .bind(&stream_id_clone)
                                        .execute(&state_clone.db)
                                        .await;
                                        let _ = channel_clone.send(StreamEvent::StreamEnded {
                                            reason: "host ended".to_string(),
                                        });
                                    }
                                    StreamClientEvent::ChatMessage { message } => {
                                        if !message.trim().is_empty() {
                                            let payload = StreamEvent::ChatMessage {
                                                user_id: current_clone.id.clone(),
                                                username: current_clone.display_name.clone(),
                                                message,
                                                timestamp: OffsetDateTime::now_utc()
                                                    .format(&Rfc3339)
                                                    .unwrap_or_else(|_| "".to_string()),
                                            };
                                            let _ = channel_clone.send(payload);
                                        }
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            break;
                        }
                        Some(Ok(Message::Ping(bytes))) => {
                            let _ = ws_session.pong(&bytes).await;
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

        let _ = sqlx::query(
            "UPDATE stream_viewers SET is_active = false, left_at = now() WHERE id = $1",
        )
        .bind(viewer_id)
        .execute(&state_clone.db)
        .await;

        let _ = sqlx::query(
            "UPDATE streams SET viewer_count = GREATEST(viewer_count - 1, 0) WHERE stream_id = $1",
        )
        .bind(&stream_id_clone)
        .execute(&state_clone.db)
        .await;

        if let Some(update) = build_viewer_update(&state_clone, &stream_id_clone).await {
            let _ = channel_clone.send(update);
        }

        let _ = ws_session.close(None).await;
    });

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

fn parse_env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok()?.parse::<u64>().ok()
}

fn resolve_media_paths() -> (String, String, String, String, String) {
    let originals = std::env::var("MEDIA_ORIGINALS_PATH").ok();
    let hls = std::env::var("MEDIA_HLS_PATH").ok();
    let thumbnails = std::env::var("MEDIA_THUMBNAILS_PATH").ok();
    let subtitles = std::env::var("MEDIA_SUBTITLES_PATH").ok();
    let upload_tmp = std::env::var("MEDIA_UPLOAD_TMP_PATH").ok();

    if originals.is_some() || hls.is_some() || thumbnails.is_some() || subtitles.is_some() {
        let originals_path = originals.unwrap_or_else(|| "/var/media/originals".to_string());
        let hls_path = hls.unwrap_or_else(|| "/var/media/hls".to_string());
        let thumbnails_path = thumbnails.unwrap_or_else(|| "/var/media/thumbnails".to_string());
        let subtitles_path = subtitles.unwrap_or_else(|| "/var/media/subtitles".to_string());
        let upload_tmp_path = upload_tmp.unwrap_or_else(|| "/var/media/uploads".to_string());
        return (
            originals_path,
            hls_path,
            thumbnails_path,
            subtitles_path,
            upload_tmp_path,
        );
    }

    if let Ok(root) = std::env::var("WATCH_MEDIA_ROOT") {
        let root_path = Path::new(&root);
        let originals_path = root_path.join("originals");
        let hls_path = root_path.join("hls");
        let thumbnails_path = root_path.join("thumbnails");
        let subtitles_path = root_path.join("subtitles");
        let upload_tmp_path = root_path.join("uploads");
        return (
            originals_path.to_string_lossy().to_string(),
            hls_path.to_string_lossy().to_string(),
            thumbnails_path.to_string_lossy().to_string(),
            subtitles_path.to_string_lossy().to_string(),
            upload_tmp_path.to_string_lossy().to_string(),
        );
    }

    (
        "/var/media/originals".to_string(),
        "/var/media/hls".to_string(),
        "/var/media/thumbnails".to_string(),
        "/var/media/subtitles".to_string(),
        "/var/media/uploads".to_string(),
    )
}

fn parse_max_upload_bytes() -> u64 {
    if let Some(bytes) = parse_env_u64("MAX_UPLOAD_BYTES") {
        return bytes;
    }

    if let Some(gb) = parse_env_u64("MAX_UPLOAD_GB") {
        return gb.saturating_mul(1024 * 1024 * 1024);
    }

    if let Some(mb) = parse_env_u64("WATCH_MAX_UPLOAD_MB") {
        return mb.saturating_mul(1024 * 1024);
    }

    10 * 1024 * 1024 * 1024
}

fn parse_allowed_formats() -> Vec<String> {
    if let Ok(value) = std::env::var("ALLOWED_VIDEO_FORMATS") {
        let list = value
            .split(',')
            .map(|entry| entry.trim().to_lowercase())
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>();
        if !list.is_empty() {
            return list;
        }
    }

    vec![
        "mp4".to_string(),
        "mkv".to_string(),
        "avi".to_string(),
        "mov".to_string(),
        "webm".to_string(),
    ]
}

fn ensure_media_dirs(config: &AppConfig) -> std::io::Result<()> {
    std_fs::create_dir_all(&config.media_originals_path)?;
    std_fs::create_dir_all(&config.media_hls_path)?;
    std_fs::create_dir_all(&config.media_thumbnails_path)?;
    std_fs::create_dir_all(&config.media_subtitles_path)?;
    std_fs::create_dir_all(&config.upload_tmp_path)?;
    Ok(())
}

fn path_for_ffmpeg(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_safe_segment(value: &str) -> bool {
    !(value.contains("..") || value.contains('/') || value.contains('\\'))
}

fn current_user_id(session: &Session) -> Result<Uuid, HttpResponse> {
    let user = match session.get::<UserProfile>("user") {
        Ok(Some(user)) => user,
        _ => return Err(HttpResponse::Unauthorized().finish()),
    };

    Uuid::parse_str(&user.id)
        .map_err(|_| HttpResponse::BadRequest().body("Invalid user id"))
}

fn build_stream_id(user: &UserProfile) -> String {
    if let Some(email) = user.email.as_ref() {
        return email
            .replace('@', "")
            .replace('.', "-")
            .to_lowercase();
    }

    user.id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .to_lowercase()
}

fn stream_channel(state: &AppState, stream_id: &str) -> broadcast::Sender<StreamEvent> {
    let mut channels = state.stream_channels.lock().unwrap();
    if let Some(sender) = channels.get(stream_id) {
        return sender.clone();
    }

    let (sender, _) = broadcast::channel(200);
    channels.insert(stream_id.to_string(), sender.clone());
    sender
}

async fn build_viewer_update(state: &AppState, stream_id: &str) -> Option<StreamEvent> {
    let rows = sqlx::query_as::<_, StreamViewerInfoRow>(
        "SELECT sv.user_id, u.email \
         FROM stream_viewers sv \
         JOIN users u ON u.id = sv.user_id \
         WHERE sv.stream_id = $1 AND sv.is_active = true \
         ORDER BY sv.joined_at ASC",
    )
    .bind(stream_id)
    .fetch_all(&state.db)
    .await
    .ok()?;

    let viewers = rows
        .into_iter()
        .map(|row| Viewer {
            id: row.user_id,
            display_name: row.email,
        })
        .collect::<Vec<_>>();

    let count = viewers.len() as i32;
    Some(StreamEvent::ViewerUpdate { count, viewers })
}

fn hls_root_for(config: &AppConfig, movie_id: &str) -> Option<PathBuf> {
    if !is_safe_segment(movie_id) {
        return None;
    }
    Some(Path::new(&config.media_hls_path).join(movie_id))
}

fn camera_hls_root_for(config: &AppConfig, stream_id: &str) -> Option<PathBuf> {
    if !is_safe_segment(stream_id) {
        return None;
    }
    Some(
        Path::new(&config.media_hls_path).join(format!("camera-{}", stream_id)),
    )
}

fn m3u8_mime() -> mime::Mime {
    "application/vnd.apple.mpegurl"
        .parse()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM)
}

fn ts_mime() -> mime::Mime {
    "video/mp2t"
        .parse()
        .unwrap_or(mime::APPLICATION_OCTET_STREAM)
}

fn resolve_ffmpeg_path(config: &AppConfig) -> String {
    config
        .ffmpeg_path
        .clone()
        .unwrap_or_else(|| "ffmpeg".to_string())
}

fn resolve_ffprobe_path(config: &AppConfig) -> String {
    if let Some(ffmpeg_path) = config.ffmpeg_path.as_ref() {
        let ffmpeg_path = Path::new(ffmpeg_path);
        if let Some(parent) = ffmpeg_path.parent() {
            let probe_name = if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" };
            let candidate = parent.join(probe_name);
            if std_fs::metadata(&candidate).is_ok() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    "ffprobe".to_string()
}

async fn get_duration_seconds(config: &AppConfig, input_path: &Path) -> Option<i32> {
    let ffprobe = resolve_ffprobe_path(config);
    let output = Command::new(ffprobe)
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("format=duration")
        .arg("-of")
        .arg("default=nw=1:nk=1")
        .arg(input_path)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let trimmed = raw.trim();
    let seconds = trimmed.parse::<f64>().ok()?;
    let rounded = seconds.round() as i64;
    Some(rounded.clamp(0, i32::MAX as i64) as i32)
}

async fn run_ffmpeg_transcode(
    config: &AppConfig,
    input_path: &Path,
    output_dir: &Path,
    movie_id: Uuid,
) -> Result<(), String> {
    fs::create_dir_all(output_dir)
        .await
        .map_err(|_| "Failed to create HLS output directory".to_string())?;
    for variant in 0..4 {
        let variant_dir = output_dir.join(format!("v{}", variant));
        fs::create_dir_all(&variant_dir)
            .await
            .map_err(|_| "Failed to create HLS variant directory".to_string())?;
    }

    let ffmpeg = resolve_ffmpeg_path(config);
    let output_dir_path = path_for_ffmpeg(output_dir);
    let segment_pattern = format!("{}/v%v/segment%03d.ts", output_dir_path);
    let playlist_pattern = format!("{}/v%v/playlist.m3u8", output_dir_path);

    let output = Command::new(ffmpeg)
        .arg("-y")
        .arg("-i")
        .arg(input_path)
        .arg("-filter_complex")
        .arg("[0:v]split=4[v1][v2][v3][v4];[v1]scale=1920:1080[v1out];[v2]scale=1280:720[v2out];[v3]scale=854:480[v3out];[v4]scale=640:360[v4out]")
        .arg("-map")
        .arg("[v1out]")
        .arg("-c:v:0")
        .arg("libx264")
        .arg("-b:v:0")
        .arg("5000k")
        .arg("-map")
        .arg("[v2out]")
        .arg("-c:v:1")
        .arg("libx264")
        .arg("-b:v:1")
        .arg("3000k")
        .arg("-map")
        .arg("[v3out]")
        .arg("-c:v:2")
        .arg("libx264")
        .arg("-b:v:2")
        .arg("1500k")
        .arg("-map")
        .arg("[v4out]")
        .arg("-c:v:3")
        .arg("libx264")
        .arg("-b:v:3")
        .arg("800k")
        .arg("-map")
        .arg("0:a?")
        .arg("-c:a:0")
        .arg("aac")
        .arg("-b:a:0")
        .arg("128k")
        .arg("-map")
        .arg("0:a?")
        .arg("-c:a:1")
        .arg("aac")
        .arg("-b:a:1")
        .arg("128k")
        .arg("-map")
        .arg("0:a?")
        .arg("-c:a:2")
        .arg("aac")
        .arg("-b:a:2")
        .arg("128k")
        .arg("-map")
        .arg("0:a?")
        .arg("-c:a:3")
        .arg("aac")
        .arg("-b:a:3")
        .arg("128k")
        .arg("-var_stream_map")
        .arg("v:0,a:0 v:1,a:1 v:2,a:2 v:3,a:3")
        .arg("-master_pl_name")
        .arg("master.m3u8")
        .arg("-f")
        .arg("hls")
        .arg("-hls_time")
        .arg("10")
        .arg("-hls_playlist_type")
        .arg("vod")
        .arg("-hls_segment_filename")
        .arg(segment_pattern)
        .arg(playlist_pattern)
        .output()
        .await
        .map_err(|_| "Failed to start FFmpeg".to_string())?;

    let log_path = output_dir.join("transcode.log");
    let mut log_text = String::new();
    log_text.push_str("FFmpeg output\n");
    log_text.push_str(&String::from_utf8_lossy(&output.stdout));
    log_text.push_str("\nFFmpeg errors\n");
    log_text.push_str(&String::from_utf8_lossy(&output.stderr));
    let _ = fs::write(&log_path, log_text).await;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!("FFmpeg failed for movie {}", movie_id))
    }
}

async fn run_thumbnail_extract(
    config: &AppConfig,
    input_path: &Path,
    movie_id: Uuid,
) -> Result<(), String> {
    let ffmpeg = resolve_ffmpeg_path(config);
    let thumbnail_path = Path::new(&config.media_thumbnails_path)
        .join(format!("{}.jpg", movie_id));

    let output = Command::new(ffmpeg)
        .arg("-y")
        .arg("-i")
        .arg(input_path)
        .arg("-ss")
        .arg("00:00:05")
        .arg("-vframes")
        .arg("1")
        .arg(thumbnail_path)
        .output()
        .await
        .map_err(|_| "Failed to extract thumbnail".to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err("Thumbnail extraction failed".to_string())
    }
}

async fn transcoding_worker(state: web::Data<AppState>, config: AppConfig) {
    let poll_interval = Duration::from_secs(5);

    loop {
        let pending = sqlx::query_as::<_, PendingMovie>(
            "SELECT id, filename FROM movies WHERE transcoding_status = 'pending' ORDER BY upload_date ASC LIMIT 1",
        )
        .fetch_optional(&state.db)
        .await;

        let pending = match pending {
            Ok(Some(movie)) => movie,
            Ok(None) => {
                sleep(poll_interval).await;
                continue;
            }
            Err(_) => {
                sleep(poll_interval).await;
                continue;
            }
        };

        let updated = sqlx::query(
            "UPDATE movies SET transcoding_status = 'processing' WHERE id = $1 AND transcoding_status = 'pending'",
        )
        .bind(pending.id)
        .execute(&state.db)
        .await
        .map(|result| result.rows_affected())
        .unwrap_or(0);

        if updated == 0 {
            continue;
        }

        let input_path = Path::new(&config.media_originals_path).join(&pending.filename);
        let output_dir = Path::new(&config.media_hls_path).join(pending.id.to_string());
        let hls_path = format!("/hls/{}/master.m3u8", pending.id);

        let transcode_result = run_ffmpeg_transcode(&config, &input_path, &output_dir, pending.id).await;
        let thumbnail_result = run_thumbnail_extract(&config, &input_path, pending.id).await;
        let duration_seconds = get_duration_seconds(&config, &input_path).await;

        let success = transcode_result.is_ok();
        if success {
            let _ = sqlx::query(
                "UPDATE movies SET transcoding_status = 'ready', hls_path = $2, duration_seconds = $3 WHERE id = $1",
            )
            .bind(pending.id)
            .bind(hls_path)
            .bind(duration_seconds)
            .execute(&state.db)
            .await;
        } else {
            let _ = sqlx::query(
                "UPDATE movies SET transcoding_status = 'failed' WHERE id = $1",
            )
            .bind(pending.id)
            .execute(&state.db)
            .await;
        }

        if thumbnail_result.is_err() {
            // Best-effort: do not fail the whole transcode if thumbnail fails.
        }
    }
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
    let (media_originals_path, media_hls_path, media_thumbnails_path, media_subtitles_path, upload_tmp_path) =
        resolve_media_paths();
    let max_upload_bytes = parse_max_upload_bytes();
    let allowed_video_formats = parse_allowed_formats();
    let ffmpeg_path = std::env::var("FFMPEG_PATH").ok();

    AppConfig {
        frontend_url,
        google_client_id,
        google_client_secret,
        google_redirect_url,
        session_secure,
        media_originals_path,
        media_hls_path,
        media_thumbnails_path,
        media_subtitles_path,
        upload_tmp_path,
        max_upload_bytes,
        allowed_video_formats,
        ffmpeg_path,
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = dotenvy::dotenv();
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let config = load_config();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");
    ensure_media_dirs(&config)?;
    let session_key = build_session_key();
    let (broadcaster, _) = broadcast::channel(200);
    let state = web::Data::new(AppState {
        users: Mutex::new(seed_users()),
        online: Mutex::new(HashSet::new()),
        messages: Mutex::new(HashMap::new()),
        broadcaster,
        db,
        stream_channels: Mutex::new(HashMap::new()),
    });
    let worker_state = state.clone();
    let worker_config = config.clone();
    tokio::spawn(async move {
        transcoding_worker(worker_state, worker_config).await;
    });

    HttpServer::new(move || {
        let cors = Cors::default()
            .allowed_origin(&config.frontend_url)
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
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
                .service(upload_movie)
                .service(list_movies)
                .service(get_movie)
                .service(update_movie)
                .service(delete_movie)
                    .service(hls_master)
                    .service(hls_variant_playlist)
                    .service(hls_segment)
                        .service(create_stream)
                        .service(list_streams)
                        .service(get_stream)
                        .service(start_stream)
                        .service(end_stream)
                        .service(list_stream_viewers)
                            .service(start_camera_stream)
            .service(list_users)
            .service(get_messages)
            .service(ws)
                            .service(ws_stream)
                            .service(ws_camera)
                            .service(camera_hls_master)
                            .service(camera_hls_segment)
    })
    .bind(bind_addr)?
    .run()
    .await
}
