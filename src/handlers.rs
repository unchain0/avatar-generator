use crate::models::{
    derive_seed, random_base_seed, validate_generate_request, AvatarRecord, AvatarView,
    GenerateRequest, GenerateResponse,
};
use crate::upstream::{avatar_url, fetch_avatar};
use crate::AppState;
use askama::Template;
use axum::{
    extract::{ConnectInfo, Path as AxumPath, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE, SET_COOKIE},
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
use zip::write::FileOptions;
use zip::CompressionMethod;

const SESSION_COOKIE: &str = "avatar_session_id";
const LANG_COOKIE: &str = "lang";
const DEFAULT_LANG: &str = "pt-br";
const SESSION_COOKIE_MAX_AGE: i64 = crate::models::MAX_TTL_DAYS * 24 * 60 * 60;

#[derive(Clone, Copy, PartialEq, Eq)]
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
    subtitle: &'static str,
    count_label: &'static str,
    seed_label: &'static str,
    seed_placeholder: &'static str,
    set_label: &'static str,
    background_label: &'static str,
    bg_none: &'static str,
    size_label: &'static str,
    ttl_label: &'static str,
    generate_btn: &'static str,
    download_all_btn: &'static str,
    clear_all_btn: &'static str,
    no_avatars: &'static str,
    avatar_alt: &'static str,
    seed_label_short: &'static str,
    set_label_short: &'static str,
    bg_label_short: &'static str,
    expires_label: &'static str,
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
            subtitle: "Generate robot avatars from a seeded RoboHash library.",
            count_label: "Count (1-50)",
            seed_label: "Custom seed (optional)",
            seed_placeholder: "Leave blank for a random seed",
            set_label: "Set",
            background_label: "Background",
            bg_none: "None",
            size_label: "Size",
            ttl_label: "Expires in (days)",
            generate_btn: "Generate",
            download_all_btn: "Download all (ZIP)",
            clear_all_btn: "Clear all",
            no_avatars: "No avatars yet. Generate a batch to get started.",
            avatar_alt: "Avatar",
            seed_label_short: "Seed",
            set_label_short: "Set",
            bg_label_short: "BG",
            expires_label: "Expires",
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
            subtitle: "Gere avatares de robô a partir de uma biblioteca RoboHash com seed.",
            count_label: "Quantidade (1-50)",
            seed_label: "Seed personalizada (opcional)",
            seed_placeholder: "Deixe em branco para uma seed aleatória",
            set_label: "Conjunto",
            background_label: "Fundo",
            bg_none: "Nenhum",
            size_label: "Tamanho",
            ttl_label: "Expira em (dias)",
            generate_btn: "Gerar",
            download_all_btn: "Baixar tudo (ZIP)",
            clear_all_btn: "Limpar tudo",
            no_avatars: "Nenhum avatar ainda. Gere um lote para começar.",
            avatar_alt: "Avatar",
            seed_label_short: "Seed",
            set_label_short: "Conjunto",
            bg_label_short: "Fundo",
            expires_label: "Expira",
            download: "Baixar",
            delete: "Excluir",
            confirm_delete: "Excluir este avatar?",
            confirm_clear: "Limpar todos os avatares desta sessão? Isso não pode ser desfeito.",
            msg_generating: "Gerando avatares...",
            msg_generated: "Avatares gerados.",
            msg_deleted: "Avatar excluído.",
            msg_cleared: "Sessão limpa.",
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
    cookie_value(headers, SESSION_COOKIE).unwrap_or_else(|| Uuid::new_v4().to_string())
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
            // Release only the slots that were not already transferred to
            // completed via per-save decrements during the loop.
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
        let url = avatar_url(&state.config.robohash_base, &seed, &options);
        let client = state.client.clone();
        let semaphore = state.semaphore.clone();
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            let result = fetch_avatar(&client, &url).await;
            (index, seed, result)
        });
    }

    // Drain tasks as they complete, saving each avatar immediately to avoid
    // buffering all fetched bytes in memory concurrently. Decrement the
    // pending reservation after each successful save so that concurrent
    // requests in the same session are not spuriously rejected (the pending
    // count reflects in-flight slots, not the full requested count).
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((index, seed, Ok(bytes))) => {
                let record = AvatarRecord {
                    id: Uuid::new_v4().to_string(),
                    seed,
                    options,
                    created_at: Utc::now(),
                    expires_at,
                    session_id: session.clone(),
                };
                let view = record.view();
                match state.store.save(record, &bytes) {
                    Ok(()) => {
                        // Transfer one pending slot to "completed" as the avatar
                        // is now persisted.
                        guard.remaining = guard.remaining.saturating_sub(1);
                        state.store.release_reservation(&session, 1);
                        results.push((index as usize, view));
                    }
                    Err(e) => error!("failed to persist avatar: {e}"),
                }
            }
            Ok((index, seed, Err(e))) => warn!("avatar {index} ({seed}) failed: {e}"),
            Err(e) => error!("avatar task panicked: {e}"),
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
        Ok(_) => Json(serde_json::json!({ "success": true })).into_response(),
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
    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(records.len());
    for record in &records {
        match state.store.read_image(&record.id) {
            Ok(Some(bytes)) => {
                total_size += bytes.len();
                if total_size > crate::models::MAX_ARCHIVE_BYTES {
                    return (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(serde_json::json!({
                            "error": "archive too large"
                        })),
                    )
                        .into_response();
                }
                entries.push((record.id.clone(), bytes));
            }
            Ok(None) => {}
            Err(e) => {
                error!("failed to read {} for zip: {e}", record.id);
            }
        }
    }

    if entries.is_empty() {
        return (StatusCode::NOT_FOUND, "no avatars in this session").into_response();
    }

    let result = tokio::task::spawn_blocking(move || {
        let cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(cursor);
        let options = FileOptions::default().compression_method(CompressionMethod::Stored);

        for (id, bytes) in &entries {
            if let Err(e) = zip.start_file(format!("{id}.png"), options) {
                error!("failed to add {id} to zip: {e}");
                continue;
            }
            if let Err(e) = zip.write_all(bytes) {
                error!("failed to write {id} to zip: {e}");
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
