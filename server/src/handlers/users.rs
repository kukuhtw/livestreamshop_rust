// server/src/handlers/users.rs
// server/src/handlers/users.rs

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use cookie::SameSite;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;
use password_hash::{PasswordHash, PasswordVerifier, PasswordHasher, SaltString};
use argon2::{Argon2};
use rand::rngs::OsRng;

use crate::{get_user_from_cookie, ensure_admin, AppState};



#[derive(Deserialize)]
struct ChangePassReq {
    old_password: String,
    new_password: String,
}

async fn api_admin_logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> impl IntoResponse {
    // hapus session di DB + clear cookie
    if let Some(c) = jar.get(&state.cookie_name) {
        let sid = c.value().to_string();
        let _ = sqlx::query!("DELETE FROM sessions WHERE sid = ?", sid)
            .execute(&state.db).await;
    }
    let cookie = Cookie::build((state.cookie_name.clone(), ""))
    .path("/")
    .http_only(true)
    .same_site(SameSite::Lax)
    .max_age(cookie::time::Duration::seconds(0))
    .build();


    let jar2 = jar.remove(cookie);
    (jar2, axum::http::StatusCode::OK)
}

async fn api_admin_change_password(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ChangePassReq>,
) -> impl IntoResponse {
    // pastikan admin login
    if let Some(u) = ensure_admin(&jar, &state).await {
        // ambil hash sekarang
        let row = sqlx::query!(
            "SELECT password_hash FROM users WHERE id=? LIMIT 1",
            u.id
        ).fetch_one(&state.db).await;

        if let Ok(r) = row {
            if let Some(stored) = r.password_hash {
                // verifikasi old password
                let parsed = PasswordHash::new(&stored).map_err(|_| ()) ;
                if parsed.is_err() {
                    return (axum::http::StatusCode::BAD_REQUEST, "Hash invalid").into_response();
                }
                let parsed = parsed.unwrap();
                if Argon2::default().verify_password(req.old_password.as_bytes(), &parsed).is_err() {
                    return (axum::http::StatusCode::UNAUTHORIZED, "Password lama salah").into_response();
                }
                // hash password baru
                if req.new_password.len() < 6 {
                    return (axum::http::StatusCode::BAD_REQUEST, "Minimal 6 karakter").into_response();
                }
                let salt = SaltString::generate(&mut OsRng);
                let new_hash = Argon2::default()
                    .hash_password(req.new_password.as_bytes(), &salt)
                    .map_err(|_| ()) .unwrap()
                    .to_string();

                let _ = sqlx::query!(
                    "UPDATE users SET password_hash=? WHERE id=?",
                    new_hash, u.id
                ).execute(&state.db).await;

                return axum::http::StatusCode::OK.into_response();
            }
        }
        (axum::http::StatusCode::BAD_REQUEST, "User tidak punya password hash").into_response()
    } else {
        (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/ensure-viewer", post(api_ensure_viewer))
        .route("/api/me", get(api_me))
        .route("/api/user/profile", post(api_user_profile)) // ← NEW
        .route("/api/admin/logout", post(api_admin_logout))
        .route("/api/admin/change-password", post(api_admin_change_password))

}

/* ------------ Models ------------ */

#[derive(Deserialize)]
struct ProfileIn {
    name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
}

#[derive(Serialize)]
struct MeOut {
    user: Option<crate::User>,
}

/* ------------ Handlers ------------ */

async fn api_ensure_viewer(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    // sudah punya session?
    if let Some((_jar, Some(_u))) = Some(get_user_from_cookie(&jar, &state).await) {
        return Json(json!({"ok": true, "role": "viewer_or_admin"})).into_response();
    }

    // buat viewer + session
    let name = format!("viewer-{}", &Uuid::new_v4().to_string()[..8]);
    let res = sqlx::query!("INSERT INTO users(role,name) VALUES('viewer',?)", name)
        .execute(&state.db)
        .await
        .unwrap();
    let user_id = res.last_insert_id() as i64;

    let sid = Uuid::new_v4().to_string();
    let _ = sqlx::query!("INSERT INTO sessions(sid,user_id) VALUES(?,?)", sid, user_id)
        .execute(&state.db)
        .await
        .unwrap();

    let mut cookie = Cookie::new(state.cookie_name.clone(), sid);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(SameSite::Lax);
    let jar = jar.add(cookie);

    (jar, Json(json!({"ok": true, "role": "viewer"}))).into_response()
}

async fn api_me(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    let (_, user) = get_user_from_cookie(&jar, &state).await;
    Json(MeOut { user }).into_response()
}

/// POST /api/user/profile
/// Body: { name?, email?, phone? }
/// Update profil untuk user pada sesi aktif. Semua field opsional.
/// Jika tidak ada perubahan, tetap balas ok:true.
async fn api_user_profile(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(inp): Json<ProfileIn>,
) -> Response {
    // pastikan ada user dari cookie
    let (_, user) = get_user_from_cookie(&jar, &state).await;
    let Some(u) = user else {
        return (axum::http::StatusCode::UNAUTHORIZED, "login first").into_response();
    };

    // ambil nilai lama (untuk fallback jika body None)
    let row = sqlx::query("SELECT name, email, phone FROM users WHERE id=?")
        .bind(u.id)
        .fetch_optional(&state.db)
        .await
        .unwrap();

    if row.is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "user not found").into_response();
    }
    let row = row.unwrap();
    let cur_name: String = row.get::<String, _>("name");
    let cur_email: Option<String> = row.get::<Option<String>, _>("email");
    let cur_phone: Option<String> = row.get::<Option<String>, _>("phone");

    // sanitasi ringan + fallback
    let mut name = inp.name.unwrap_or(cur_name);
    let mut email = inp.email.or(cur_email);
    let mut phone = inp.phone.or(cur_phone);

    // trim & batasi panjang
    name.truncate(120);
    if let Some(e) = &mut email { e.truncate(190); }
    if let Some(p) = &mut phone { p.truncate(60); }

    // update
    let _ = sqlx::query!(
        "UPDATE users SET name=?, email=?, phone=? WHERE id=?",
        name,
        email,
        phone,
        u.id
    )
    .execute(&state.db)
    .await
    .unwrap();

    Json(json!({"ok": true})).into_response()
}
