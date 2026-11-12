use crate::models::{
    derive_seed, random_base_seed, validate_generate_request, AvatarOptions, AvatarRecord,
    AvatarView, Background, GenerateRequest, GenerateResponse, RoboSet,
};
use crate::upstream::{avatar_url, fetch_avatar};
use crate::AppState;
use askama::Template;
use axum::{
    extract::{ConnectInfo, Path as AxumPath, Query, State},
    http::{
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{Html, IntoResponse, Response},
    Json,
};
use chrono::{Duration as ChronoDuration, Utc};
use std::io::{Cursor, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use tokio::task::JoinSet;
use tracing::{error, warn};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

const SESSION_COOKIE: &str = "avatar_session_id";
const LANG_COOKIE: &str = "lang";
const DEFAULT_LANG: &str = "pt-br";
const SESSION_COOKIE_MAX_AGE: i64 = crate::models::MAX_TTL_DAYS * 24 * 60 * 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Lang {
    PtBr,
    En,
}

impl Lang {
    fn parse(value: &str) -> Self {
        if value.eq_ignore_ascii_case("en") {
            Lang::En
        } else {
            Lang::PtBr
        }
    }
}

struct Texts {
    app_title: &'static str,
    page_title: &'static str,
    title: &'static str,
    lede: &'static str,
    count_label: &'static str,
    seed_label: &'static str,
    seed_placeholder: &'static str,
    seed_note: &'static str,
    set_label: &'static str,
    background_label: &'static str,
    preview_label: &'static str,
    preview_unavailable: &'static str,
    skip_to_content: &'static str,
    gallery_title: &'static str,
    empty_title: &'static str,
    empty_hint: &'static str,
    footer_note: &'static str,
    bg_none: &'static str,
    size_label: &'static str,
    ttl_label: &'static str,
    generate_btn: &'static str,
    download_all_btn: &'static str,
    clear_all_btn: &'static str,
    avatar_alt: &'static str,
    download: &'static str,
    delete: &'static str,
    confirm_delete: &'static str,
    confirm_clear: &'static str,
    msg_generating: &'static str,
    msg_generated: &'static str,
    msg_deleted: &'static str,
    msg_cleared: &'static str,
    msg_error: &'static str,
    msg_invalid: &'static str,
}

fn texts(lang: Lang) -> Texts {
    match lang {
        Lang::En => Texts {
            app_title: "RoboHash Avatar Generator",
            page_title: "RoboHash Avatars",
            title: "RoboHash avatar generator",
            lede: "Seeded robot avatars from a local RoboHash library. Pick a set, a background and a size.",
            count_label: "How many",
            seed_label: "Seed",
            seed_placeholder: "Blank means random",
            seed_note: "A blank seed creates a random avatar.",
            set_label: "Set",
            background_label: "Background",
            preview_label: "Preview",
            preview_unavailable: "Preview unavailable",
            skip_to_content: "Skip to content",
            gallery_title: "Saved avatars",
            empty_title: "Nothing saved yet",
            empty_hint: "Generate a batch to see the files here.",
            footer_note: "Artwork from RoboHash, rendered on this server.",
            bg_none: "None",
            size_label: "Size",
            ttl_label: "Keep for (days)",
            generate_btn: "Generate avatars",
            download_all_btn: "Download ZIP",
            clear_all_btn: "Clear all",
            avatar_alt: "Avatar",
            download: "Download",
            delete: "Delete",
            confirm_delete: "Delete this avatar?",
            confirm_clear: "Clear all avatars in this session? This cannot be undone.",
            msg_generating: "Generating avatars...",
            msg_generated: "Avatars generated.",
            msg_deleted: "Avatar deleted.",
            msg_cleared: "Session cleared.",
            msg_error: "Something went wrong. Please try again.",
            msg_invalid: "Enter a count between 1 and 50.",
        },
        Lang::PtBr => Texts {
            app_title: "Gerador de Avatares RoboHash",
            page_title: "Avatares RoboHash",
            title: "Gerador de avatares RoboHash",
            lede: "Avatares de rob\u{f4} gerados por seed nesta m\u{e1}quina. Escolha conjunto, fundo e tamanho.",
            count_label: "Quantidade",
            seed_label: "Seed",
            seed_placeholder: "Vazio gera aleat\u{f3}ria",
            seed_note: "Seed vazia cria um avatar aleat\u{f3}rio.",
            set_label: "Conjunto",
            background_label: "Fundo",
            preview_label: "Pr\u{e9}via",
            preview_unavailable: "Pr\u{e9}via indispon\u{ed}vel",
            skip_to_content: "Pular para o conte\u{fa}do",
            gallery_title: "Avatares salvos",
            empty_title: "Nada salvo ainda",
            empty_hint: "Gere um lote para ver os arquivos aqui.",
            footer_note: "Arte do RoboHash, renderizada neste servidor.",
            bg_none: "Nenhum",
            size_label: "Tamanho",
            ttl_label: "Guardar por (dias)",
            generate_btn: "Gerar avatares",
            download_all_btn: "Baixar ZIP",
            clear_all_btn: "Limpar tudo",
            avatar_alt: "Avatar",
            download: "Baixar",
            delete: "Excluir",
            confirm_delete: "Excluir este avatar?",
            confirm_clear:
                "Limpar todos os avatares desta sess\u{e3}o? Isso n\u{e3}o pode ser desfeito.",
            msg_generating: "Gerando avatares...",
            msg_generated: "Avatares gerados.",
            msg_deleted: "Avatar exclu\u{ed}do.",
            msg_cleared: "Sess\u{e3}o limpa.",
            msg_error: "Algo deu errado. Tente novamente.",
            msg_invalid: "Informe uma quantidade entre 1 e 50.",
        },
    }
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    avatars: Vec<AvatarView>,
    lang: String,
    default_ttl: i64,
    t: Texts,
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;
    cookie_header.split(';').find_map(|pair| {
        let mut parts = pair.trim().splitn(2, '=');
        let key = parts.next()?.trim();
        let value = parts.next()?.trim();
        (key == name && !value.is_empty()).then(|| value.to_string())
    })
}

fn session_id(headers: &HeaderMap) -> String {
    cookie_value(headers, SESSION_COOKIE).unwrap_or_else(|| Uuid::now_v7().to_string())
}

fn lang(headers: &HeaderMap) -> Lang {
    Lang::parse(&cookie_value(headers, LANG_COOKIE).unwrap_or_else(|| DEFAULT_LANG.to_string()))
}

fn client_key(headers: &HeaderMap, peer: SocketAddr, trust_forwarded: bool) -> String {
    if trust_forwarded {
        if let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            return forwarded;
        }
        if let Some(real) = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
        {
            return real;
        }
    }
    peer.ip().to_string()
}

fn session_cookie(session: &str) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={session}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_COOKIE_MAX_AGE}"
    ))
    .expect("session cookie is ASCII")
}

fn safe_join(base: &Path, rel: &str) -> Option<PathBuf> {
    let candidate = base.join(rel);
    let canonical_base = base.canonicalize().ok()?;
    let canonical = candidate.canonicalize().ok()?;
    canonical.starts_with(&canonical_base).then_some(canonical)
}

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn index(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = session_id(&headers);
    let current_lang = lang(&headers);
    let lang_code = match current_lang {
        Lang::En => "en",
        Lang::PtBr => "pt-br",
    };

    let avatars: Vec<AvatarView> = state
        .store
        .list_session(&session)
        .iter()
        .map(AvatarRecord::view)
        .collect();

    let template = IndexTemplate {
        avatars,
        lang: lang_code.to_string(),
        default_ttl: state.config.default_ttl_days,
        t: texts(current_lang),
    };

    let body = template.render().unwrap_or_else(|e| {
        error!("failed to render index: {e}");
        String::new()
    });

    let mut response = Html(body).into_response();
    response
        .headers_mut()
        .insert(SET_COOKIE, session_cookie(&session));
    response
}

pub async fn generate(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(payload): Json<GenerateRequest>,
) -> Response {
    let session = session_id(&headers);

    if !state.rate.check(&client_key(
        &headers,
        peer,
        state.config.trust_forwarded_headers,
    )) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": "rate limit exceeded" })),
        )
            .into_response();
    }

    let validated = match validate_generate_request(&payload, state.config.default_ttl_days) {
        Ok(v) => v,
        Err(message) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": message })),
            )
                .into_response();
        }
    };

    if let Err(e) = state
        .store
        .reserve_capacity(&session, validated.count as usize)
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response();
    }

    struct ReservationGuard {
        session: String,
        remaining: usize,
        store: std::sync::Arc<crate::store::Store>,
    }
    impl Drop for ReservationGuard {
        fn drop(&mut self) {
            self.store
                .release_reservation(&self.session, self.remaining);
        }
    }

    let mut guard = ReservationGuard {
        session: session.clone(),
        remaining: validated.count as usize,
        store: state.store.clone(),
    };

    let base_seed = validated.seed.clone().unwrap_or_else(random_base_seed);
    let options = validated.options;

    let expires_at = Utc::now() + ChronoDuration::days(validated.ttl_days);
    let mut results: Vec<(usize, AvatarView)> = Vec::new();

    let mut tasks = JoinSet::new();
    for index in 0..validated.count {
        let seed = derive_seed(&base_seed, index);
        let url = match avatar_url(&state.config.robohash_base, &seed, &options) {
            Ok(url) => url,
            Err(e) => {
                error!("invalid ROBOHASH_BASE_URL: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "avatar renderer misconfigured" })),
                )
                    .into_response();
            }
        };
        let client = state.client.clone();
        let semaphore = state.semaphore.clone();
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            let result = fetch_avatar(&client, &url).await;
            (index, seed, result)
        });
    }

    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((index, seed, Ok(bytes))) => {
                let record = AvatarRecord {
                    id: Uuid::now_v7().to_string(),
                    seed,
                    options,
                    created_at: Utc::now(),
                    expires_at,
                    session_id: session.clone(),
                };
                let view = record.view();
                match state.store.save(record, &bytes) {
                    Ok(()) => {
                        guard.remaining = guard.remaining.saturating_sub(1);
                        state.store.release_reservation(&session, 1);
                        results.push((index as usize, view));
                    }
                    Err(e) => error!("failed to persist avatar: {e}"),
                }
            }
            Ok((index, seed, Err(e))) => warn!("avatar {index} ({seed}) failed: {e}"),
            Err(_) => error!("avatar task panicked"),
        }
    }

    results.sort_by_key(|(index, _)| *index);

    let avatars = results.into_iter().map(|(_, v)| v).collect::<Vec<_>>();

    let _guard = guard;

    if avatars.is_empty() {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": "no avatars could be generated" })),
        )
            .into_response();
    }

    let mut response = Json(GenerateResponse {
        count: avatars.len(),
        avatars,
    })
    .into_response();
    response
        .headers_mut()
        .insert(SET_COOKIE, session_cookie(&session));
    response
}

const PREVIEW_SEED: &str = "avatar-preview";
const PREVIEW_CACHE: &str = "public, max-age=600";
const PREVIEW_MIN_SIZE: u32 = 16;
const PREVIEW_MAX_SIZE: u32 = 1024;
const PREVIEW_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(3600);
const PREVIEW_CACHE_CAPACITY: usize = 96;

type PreviewKey = (String, u32);
type PreviewEntries = std::collections::HashMap<PreviewKey, (std::time::Instant, Vec<u8>)>;

/// Rendered previews only depend on (set, background, size), so a small
/// in-process cache turns a page of eight thumbnails into one render.
pub struct PreviewCache {
    entries: std::sync::Mutex<PreviewEntries>,
}

impl Default for PreviewCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewCache {
    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn get(&self, key: &(String, u32)) -> Option<Vec<u8>> {
        let entries = self.entries.lock().ok()?;
        let (created, bytes) = entries.get(key)?;
        (created.elapsed() < PREVIEW_CACHE_TTL).then(|| bytes.clone())
    }

    fn put(&self, key: (String, u32), bytes: Vec<u8>) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= PREVIEW_CACHE_CAPACITY {
            entries.retain(|_, (created, _)| created.elapsed() < PREVIEW_CACHE_TTL);
            if entries.len() >= PREVIEW_CACHE_CAPACITY {
                entries.clear();
            }
        }
        entries.insert(key, (std::time::Instant::now(), bytes));
    }
}

#[derive(serde::Deserialize)]
pub struct PreviewQuery {
    set: String,
    background: Option<String>,
    size: Option<u32>,
}

/// Renders the UI previews through the same upstream as generation, so the
/// browser never depends on a third party and CSP can stay `self`.
pub async fn preview(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<PreviewQuery>,
) -> Response {
    let set = match RoboSet::parse(&query.set) {
        Ok(set) => set,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let background = match Background::parse(query.background.as_deref().unwrap_or("none")) {
        Ok(background) => background,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    // Previews are not stored, so they take any square size the renderer
    // supports; only the bound matters.
    let size = query.size.unwrap_or(300);
    if !(PREVIEW_MIN_SIZE..=PREVIEW_MAX_SIZE).contains(&size) {
        return (
            StatusCode::BAD_REQUEST,
            format!("invalid size {size}, allowed: {PREVIEW_MIN_SIZE}..={PREVIEW_MAX_SIZE}"),
        )
            .into_response();
    }

    let key = (format!("{}-{}", set.as_str(), background.as_str()), size);
    if let Some(bytes) = state.preview_cache.get(&key) {
        return png_preview(bytes);
    }

    // A page asks for nine previews at once, so previews get their own, larger
    // per-IP budget instead of starving generation.
    if !state.preview_rate.check(&client_key(
        &headers,
        peer,
        state.config.trust_forwarded_headers,
    )) {
        return (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
    }

    match render_preview(&state, set, background, size).await {
        Ok(bytes) => png_preview(bytes),
        Err(PreviewError::Misconfigured) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "avatar renderer misconfigured",
        )
            .into_response(),
        Err(_) => (StatusCode::BAD_GATEWAY, "preview unavailable").into_response(),
    }
}

enum PreviewError {
    Misconfigured,
    Unavailable,
    Busy,
}

fn png_preview(bytes: Vec<u8>) -> Response {
    (
        [(CONTENT_TYPE, "image/png"), (CACHE_CONTROL, PREVIEW_CACHE)],
        bytes,
    )
        .into_response()
}

async fn render_preview(
    state: &AppState,
    set: RoboSet,
    background: Background,
    size: u32,
) -> Result<Vec<u8>, PreviewError> {
    let key = (format!("{}-{}", set.as_str(), background.as_str()), size);
    if let Some(bytes) = state.preview_cache.get(&key) {
        return Ok(bytes);
    }

    let url = avatar_url(
        &state.config.robohash_base,
        PREVIEW_SEED,
        &AvatarOptions {
            set,
            background,
            size,
        },
    )
    .map_err(|e| {
        error!("invalid ROBOHASH_BASE_URL: {e}");
        PreviewError::Misconfigured
    })?;

    let _permit = state
        .semaphore
        .acquire()
        .await
        .map_err(|_| PreviewError::Busy)?;
    let bytes = fetch_avatar(&state.client, &url).await.map_err(|e| {
        warn!("preview {url} failed: {e}");
        PreviewError::Unavailable
    })?;
    state.preview_cache.put(key, bytes.clone());
    Ok(bytes)
}

/// Fills the cache with the previews the first page asks for, so the first
/// visitor after a restart does not wait for nine renders. The renderer may
/// still be starting, so each key gets a few attempts.
pub async fn warm_previews(state: AppState) {
    for set in [
        RoboSet::Set1,
        RoboSet::Set2,
        RoboSet::Set3,
        RoboSet::Set4,
        RoboSet::Set5,
    ] {
        warm_key(&state, set, Background::Bg1, 120, 3).await;
    }
    for background in [Background::Bg2, Background::None] {
        warm_key(&state, RoboSet::Set1, background, 120, 3).await;
    }
    warm_key(&state, RoboSet::Set1, Background::Bg1, 300, 3).await;
    tracing::info!("preview cache warmed");
}

async fn warm_key(
    state: &AppState,
    set: RoboSet,
    background: Background,
    size: u32,
    attempts: u32,
) {
    for attempt in 0..attempts {
        if render_preview(state, set, background, size).await.is_ok() {
            return;
        }
        if attempt + 1 < attempts {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }
    warn!("preview warm-up skipped set={set:?} {size}px");
}

pub async fn get_avatar(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> Response {
    match state.store.read_image(&id) {
        Ok(Some(bytes)) => ([(CONTENT_TYPE, "image/png")], bytes).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "avatar not found").into_response(),
        Err(e) => {
            error!("failed to read avatar {id}: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

pub async fn download(State(state): State<AppState>, AxumPath(id): AxumPath<String>) -> Response {
    match state.store.read_image(&id) {
        Ok(Some(bytes)) => (
            [
                (CONTENT_TYPE, "image/png".to_string()),
                (
                    CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{id}.png\""),
                ),
            ],
            bytes,
        )
            .into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "avatar not found").into_response(),
        Err(e) => {
            error!("failed to read avatar {id}: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

pub async fn delete_avatar(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let session = session_id(&headers);
    let owned = state
        .store
        .get(&id)
        .is_some_and(|record| record.session_id == session);

    if !owned {
        return (StatusCode::NOT_FOUND, "avatar not found").into_response();
    }

    match state.store.delete(&id) {
        Ok(true) => Json(serde_json::json!({ "success": true })).into_response(),
        Ok(false) => (StatusCode::INTERNAL_SERVER_ERROR, "failed to delete avatar").into_response(),
        Err(e) => {
            error!("failed to delete avatar {id}: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

pub async fn clear_all(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = session_id(&headers);
    match state.store.clear_session(&session) {
        Ok(count) => Json(serde_json::json!({ "success": true, "removed": count })).into_response(),
        Err(e) => {
            error!("failed to clear session {session}: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

pub async fn download_all(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = session_id(&headers);
    let records = state.store.list_session(&session);
    if records.is_empty() {
        return (StatusCode::NOT_FOUND, "no avatars in this session").into_response();
    }

    let mut total_size: usize = 0;
    let mut ids_and_paths: Vec<(String, PathBuf)> = Vec::with_capacity(records.len());
    for record in &records {
        if let Some(path) = state.store.image_path(&record.id) {
            if let Ok(metadata) = std::fs::metadata(&path) {
                let size = metadata.len() as usize;
                total_size += size;
                if total_size > crate::models::MAX_ARCHIVE_BYTES {
                    return (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(serde_json::json!({
                            "error": "archive too large"
                        })),
                    )
                        .into_response();
                }
            }
            ids_and_paths.push((record.id.clone(), path));
        }
    }

    if ids_and_paths.is_empty() {
        return (StatusCode::NOT_FOUND, "no avatars in this session").into_response();
    }

    let result = tokio::task::spawn_blocking(move || {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        for (id, path) in &ids_and_paths {
            if let Ok(bytes) = std::fs::read(path) {
                if let Err(e) = zip.start_file(format!("{id}.png"), options) {
                    error!("failed to add {id} to zip: {e}");
                    continue;
                }
                if let Err(e) = zip.write_all(&bytes) {
                    error!("failed to write {id} to zip: {e}");
                }
            } else {
                error!("failed to read {id} for zip");
            }
        }
        zip.finish().map(|c| c.into_inner())
    })
    .await;

    let data = match result {
        Ok(Ok(data)) => data,
        Ok(Err(e)) => {
            error!("failed to finish zip: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
        Err(e) => {
            error!("zip task panicked: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response();
        }
    };

    (
        [
            (CONTENT_TYPE, "application/zip".to_string()),
            (
                CONTENT_DISPOSITION,
                "attachment; filename=\"avatars.zip\"".to_string(),
            ),
        ],
        data,
    )
        .into_response()
}

pub async fn static_file(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    let Some(file_path) = safe_join(&state.config.static_dir, &path) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    if !file_path.is_file() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    match tokio::fs::read(&file_path).await {
        Ok(data) => {
            let mime = mime_guess::from_path(&file_path).first_or_octet_stream();
            ([(CONTENT_TYPE, mime.as_ref())], data).into_response()
        }
        Err(e) => {
            error!("failed to read static file {}: {e}", file_path.display());
            (StatusCode::INTERNAL_SERVER_ERROR, "internal error").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn cookie_value_returns_none_when_no_cookie_header() {
        let headers = HeaderMap::new();
        assert_eq!(cookie_value(&headers, "test"), None);
    }

    #[test]
    fn cookie_value_returns_none_when_cookie_not_found() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("other=value"));
        assert_eq!(cookie_value(&headers, "test"), None);
    }

    #[test]
    fn cookie_value_returns_value_when_found() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_static("test=value123; other=value"),
        );
        assert_eq!(cookie_value(&headers, "test"), Some("value123".to_string()));
    }

    #[test]
    fn cookie_value_handles_multiple_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("a=1; test=hello; b=2"));
        assert_eq!(cookie_value(&headers, "test"), Some("hello".to_string()));
    }

    #[test]
    fn cookie_value_returns_none_for_empty_value() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("test=; other=value"));
        assert_eq!(cookie_value(&headers, "test"), None);
    }

    #[test]
    fn session_id_returns_cookie_when_present() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            HeaderValue::from_static("avatar_session_id=test-session-123"),
        );
        assert_eq!(session_id(&headers), "test-session-123");
    }

    #[test]
    fn session_id_generates_uuid_when_missing() {
        let headers = HeaderMap::new();
        let id = session_id(&headers);
        assert!(uuid::Uuid::parse_str(&id).is_ok());
    }

    #[test]
    fn lang_parses_en() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("lang=en"));
        assert_eq!(lang(&headers), Lang::En);
    }

    #[test]
    fn lang_parses_en_case_insensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("lang=EN"));
        assert_eq!(lang(&headers), Lang::En);
    }

    #[test]
    fn lang_defaults_to_pt_br_when_missing() {
        let headers = HeaderMap::new();
        assert_eq!(lang(&headers), Lang::PtBr);
    }

    #[test]
    fn lang_defaults_to_pt_br_when_invalid() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_static("lang=fr"));
        assert_eq!(lang(&headers), Lang::PtBr);
    }

    #[test]
    fn client_key_uses_peer_when_trust_forwarded_false() {
        let headers = HeaderMap::new();
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 12345);
        let key = client_key(&headers, peer, false);
        assert_eq!(key, "1.2.3.4");
    }

    #[test]
    fn client_key_uses_x_forwarded_for_when_trust_forwarded_true() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("10.0.0.1, 10.0.0.2"),
        );
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 12345);
        let key = client_key(&headers, peer, true);
        assert_eq!(key, "10.0.0.1");
    }

    #[test]
    fn client_key_uses_x_real_ip_when_x_forwarded_for_missing() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("192.168.1.1"));
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 12345);
        let key = client_key(&headers, peer, true);
        assert_eq!(key, "192.168.1.1");
    }

    #[test]
    fn client_key_falls_back_to_peer_when_headers_invalid() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static(""));
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8)), 12345);
        let key = client_key(&headers, peer, true);
        assert_eq!(key, "5.6.7.8");
    }

    fn preview_key(size: u32) -> (String, u32) {
        ("set1-bg1".to_string(), size)
    }

    #[test]
    fn preview_cache_returns_what_it_stored() {
        let cache = PreviewCache::new();
        assert!(cache.get(&preview_key(120)).is_none());
        cache.put(preview_key(120), vec![1, 2, 3]);
        assert_eq!(cache.get(&preview_key(120)), Some(vec![1, 2, 3]));
        assert!(cache.get(&preview_key(300)).is_none());
    }

    #[test]
    fn preview_cache_stays_bounded() {
        let cache = PreviewCache::new();
        for size in 16..16 + PREVIEW_CACHE_CAPACITY as u32 + 1 {
            cache.put(preview_key(size), vec![0; 4]);
        }
        let len = cache.entries.lock().unwrap().len();
        assert!(len <= PREVIEW_CACHE_CAPACITY);
        assert_eq!(
            cache.get(&preview_key(16)),
            None,
            "oldest entries were evicted"
        );
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;
    use crate::config::Config;
    use crate::store::Store;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn state(dir: &std::path::Path, base: &str) -> AppState {
        let store = Arc::new(Store::new(dir.to_path_buf()).unwrap());
        let config = Config {
            port: 0,
            data_dir: dir.to_path_buf(),
            static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
            robohash_base: base.to_string(),
            default_ttl_days: 1,
            cleanup_interval_secs: 600,
            max_upstream_concurrency: 2,
            rate_limit_per_minute: 30,
            preview_limit_per_minute: 240,
            trust_forwarded_headers: false,
        };
        crate::build_state(config, store)
    }

    #[test]
    fn preview_cache_default_and_poisoned_lookups_are_safe() {
        let cache = PreviewCache::default();
        let key = ("set1-bg1".to_string(), 120);
        cache.put(key.clone(), vec![1]);

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = cache.entries.lock().unwrap();
            panic!("poison the preview cache");
        }));

        assert_eq!(cache.get(&key), None);
        cache.put(key, vec![2]);
    }

    #[tokio::test]
    async fn render_preview_returns_a_cached_render_without_the_network() {
        let dir = tempfile::TempDir::new().unwrap();
        let state = state(dir.path(), "http://127.0.0.1:9");
        state
            .preview_cache
            .put(("set1-none".to_string(), 80), vec![9, 9]);

        let bytes = render_preview(&state, RoboSet::Set1, Background::None, 80)
            .await
            .ok()
            .expect("cached render");
        assert_eq!(bytes, vec![9, 9]);
    }

    #[tokio::test]
    async fn warm_key_gives_up_after_its_attempts() {
        let dir = tempfile::TempDir::new().unwrap();
        let state = state(dir.path(), "not a url");
        warm_key(&state, RoboSet::Set1, Background::None, 80, 1).await;
        assert!(state
            .preview_cache
            .get(&("set1-none".to_string(), 80))
            .is_none());
    }
}
