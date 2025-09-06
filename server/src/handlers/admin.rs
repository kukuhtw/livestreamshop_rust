// server/src/handlers/admin.rs
/*
=============================================================================
Project : LiveStreamShop Rust — sell via live stream, chat & checkout on your site. 
Author  : Kukuh Tripamungkas Wicaksono (Kukuh TW)
Email   : kukuhtw@gmail.com
WhatsApp: https://wa.me/628129893706
LinkedIn: https://id.linkedin.com/in/kukuhtw
=============================================================================
*/

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::{post, get}, // ⬅️ tambahkan get
    Json, Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use cookie::SameSite;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{PasswordHash, SaltString};
use rand::rngs::OsRng;

use crate::AppState;

#[derive(Deserialize)]
struct AdminLoginReq { username: String, password: String }
#[derive(Deserialize)]
struct AdminBootstrapReq { username: String, password: String, name: String }

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/admin/exists", get(api_admin_exists)) // ⬅️ NEW
        .route("/api/admin/bootstrap", post(api_admin_bootstrap))
        .route("/api/admin/login", post(api_admin_login))
}

async fn api_admin_exists(
    State(state): State<AppState>,
) -> Response {
    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM admins LIMIT 1")
            .fetch_optional(&state.db).await.unwrap();
    Json(json!({ "exists": existing.is_some() })).into_response()
}

async fn api_admin_bootstrap(
    State(state): State<AppState>,
    Json(req): Json<AdminBootstrapReq>,
) -> Response {
    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM admins LIMIT 1")
            .fetch_optional(&state.db).await.unwrap();
    if existing.is_some() {
        return (axum::http::StatusCode::BAD_REQUEST, "Admin already exists").into_response();
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(req.password.as_bytes(), &salt)
        .unwrap()
        .to_string();

    let _ = sqlx::query("INSERT INTO admins(name,email,password_hash) VALUES(?,?,?)")
        .bind(&req.name)
        .bind(&req.username)
        .bind(&hash)
        .execute(&state.db).await.unwrap();

    let _ = sqlx::query("INSERT INTO users(role,name,email,phone,password_hash) VALUES('admin',?,?,NULL,?)")
        .bind(&req.name)
        .bind(&req.username)
        .bind(&hash)
        .execute(&state.db).await.unwrap();

    Json(json!({"ok": true})).into_response()
}

async fn api_admin_login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AdminLoginReq>,
) -> Response {
    let row = sqlx::query!(
        "SELECT id, name, email, password_hash FROM admins WHERE email=? LIMIT 1",
        req.username
    )
    .fetch_optional(&state.db).await.unwrap();

    if let Some(a) = row {
        if let Ok(parsed) = PasswordHash::new(&a.password_hash) {
            if Argon2::default().verify_password(req.password.as_bytes(), &parsed).is_ok() {
                let user_row = sqlx::query!(
                    "SELECT id FROM users WHERE email=? AND role='admin' LIMIT 1",
                    a.email
                ).fetch_optional(&state.db).await.unwrap();

                let user_id = if let Some(u) = user_row {
                    u.id
                } else {
                    sqlx::query!(
                        "INSERT INTO users(role,name,email,password_hash) VALUES('admin',?,?,?)",
                        a.name, a.email, a.password_hash
                    )
                    .execute(&state.db).await.unwrap().last_insert_id() as i64
                };

                let sid = Uuid::new_v4().to_string();
                let _ = sqlx::query!("INSERT INTO sessions(sid,user_id) VALUES(?,?)", sid, user_id)
                    .execute(&state.db).await.unwrap();

                let mut cookie = Cookie::new(state.cookie_name.clone(), sid);
                cookie.set_path("/");
                cookie.set_http_only(true);
                cookie.set_same_site(SameSite::Lax);
                let jar = jar.add(cookie);

                return (jar, Json(json!({"ok": true, "role": "admin"}))).into_response();
            }
        }
    }
    (axum::http::StatusCode::UNAUTHORIZED, "invalid credentials").into_response()
}
