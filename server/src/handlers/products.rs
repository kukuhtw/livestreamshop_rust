// server/src/handlers/products.rs

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
    extract::{Multipart, Path, State},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

use crate::{ensure_admin, AppState};

#[derive(Serialize, Deserialize)]
struct ProductOut {
    id: i64,
    name: String,
    description: Option<String>,
    image_url: Option<String>,
    price_idr: i32,
    is_active: bool,
}

#[derive(Deserialize)]
struct ProductIn {
    name: String,
    description: Option<String>,
    price_idr: i32,
    is_active: Option<bool>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/products", get(api_products_list))
        .route("/api/admin/products", post(api_admin_product_create))
        .route("/api/admin/products/:id", put(api_admin_product_update))
        .route("/api/admin/products/:id", delete(api_admin_product_delete))
        .route("/api/admin/products/:id/photo", post(api_admin_product_upload_photo))
}

async fn api_products_list(State(state): State<AppState>) -> Response {
    let rows = sqlx::query!(
        "SELECT id, name, description, image_url, price_idr, is_active
         FROM products WHERE is_active=1 ORDER BY id DESC"
    )
    .fetch_all(&state.db)
    .await
    .unwrap();

    let list: Vec<ProductOut> = rows
        .into_iter()
        .map(|r| ProductOut {
            id: r.id,
            name: r.name,
            description: r.description,
            image_url: r.image_url,
            price_idr: r.price_idr,
            is_active: r.is_active != 0,
        })
        .collect();

    Json(list).into_response()
}

async fn api_admin_product_create(
    State(state): State<AppState>,
    jar: axum_extra::extract::cookie::CookieJar,
    Json(inp): Json<ProductIn>,
) -> Response {
    if ensure_admin(&jar, &state).await.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "admin only").into_response();
    }
    let is_active = inp.is_active.unwrap_or(true);
    let _ = sqlx::query!(
        "INSERT INTO products(name,description,price_idr,is_active) VALUES(?,?,?,?)",
        inp.name,
        inp.description,
        inp.price_idr,
        if is_active { 1 } else { 0 }
    )
    .execute(&state.db)
    .await
    .unwrap();

    Json(json!({"ok": true})).into_response()
}

async fn api_admin_product_update(
    State(state): State<AppState>,
    jar: axum_extra::extract::cookie::CookieJar,
    Path(id): Path<i64>,
    Json(inp): Json<ProductIn>,
) -> Response {
    if ensure_admin(&jar, &state).await.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "admin only").into_response();
    }
    let is_active = inp.is_active.unwrap_or(true);
    let _ = sqlx::query!(
        "UPDATE products SET name=?, description=?, price_idr=?, is_active=? WHERE id=?",
        inp.name,
        inp.description,
        inp.price_idr,
        if is_active { 1 } else { 0 },
        id
    )
    .execute(&state.db)
    .await
    .unwrap();

    Json(json!({"ok": true})).into_response()
}

async fn api_admin_product_delete(
    State(state): State<AppState>,
    jar: axum_extra::extract::cookie::CookieJar,
    Path(id): Path<i64>,
) -> Response {
    if ensure_admin(&jar, &state).await.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "admin only").into_response();
    }
    let _ = sqlx::query!("DELETE FROM products WHERE id=?", id)
        .execute(&state.db)
        .await
        .unwrap();

    Json(json!({"ok": true})).into_response()
}

async fn api_admin_product_upload_photo(
    State(state): State<AppState>,
    jar: axum_extra::extract::cookie::CookieJar,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Response {
    if ensure_admin(&jar, &state).await.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "admin only").into_response();
    }

    // cek product
    let prod = sqlx::query!("SELECT id FROM products WHERE id=? LIMIT 1", id)
        .fetch_optional(&state.db)
        .await
        .unwrap();
    if prod.is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "product not found").into_response();
    }

    let mut saved_url: Option<String> = None;

    while let Some(field) = multipart.next_field().await.unwrap() {
        if field.name() != Some("photo") {
            continue;
        }

        let filename = field.file_name().unwrap_or("upload.bin").to_string();
        let content_type = field.content_type().map(|m| m.to_string()).unwrap_or_default();

        if !content_type.starts_with("image/") {
            return (axum::http::StatusCode::BAD_REQUEST, "invalid content-type").into_response();
        }

        let bytes = field.bytes().await.unwrap();
        if bytes.len() > 5 * 1024 * 1024 {
            return (axum::http::StatusCode::PAYLOAD_TOO_LARGE, "file too large").into_response();
        }

        let ext = filename.rsplit('.').next().unwrap_or("jpg");
        let safe_ext = match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => "jpg",
            "png" => "png",
            "gif" => "gif",
            "webp" => "webp",
            _ => "jpg",
        };
        let new_name = format!("p{}_{}.{}", id, &Uuid::new_v4().to_string()[..8], safe_ext);

        // simpan ke ../uploads (pastikan folder ada)
        fs::create_dir_all("../uploads").await.ok();
        let path = format!("../uploads/{}", new_name);
        let mut file = fs::File::create(&path).await.unwrap();
        file.write_all(&bytes).await.unwrap();

        let public_url = format!("/uploads/{}", new_name);
        let _ = sqlx::query!("UPDATE products SET image_url=? WHERE id=?", public_url, id)
            .execute(&state.db)
            .await
            .unwrap();

        saved_url = Some(public_url);
        break;
    }

    if let Some(url) = saved_url {
        Json(json!({ "ok": true, "image_url": url })).into_response()
    } else {
        (axum::http::StatusCode::BAD_REQUEST, "no photo field").into_response()
    }
}
