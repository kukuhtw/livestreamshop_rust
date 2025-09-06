// server/src/handlers/cart.rs
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
    extract::{Path, State},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use sqlx::{MySql, Pool};

use crate::{get_user_from_cookie, AppState};

#[derive(Deserialize)]
struct AddItemReq {
    product_id: i64,
    qty: i32,
}
#[derive(Deserialize)]
struct UpdateItemReq {
    qty: i32,
}

#[derive(Serialize)]
struct CartView {
    cart_id: i64,
    items: Vec<CartItemView>,
    subtotal: i64,
}
#[derive(Serialize)]
struct CartItemView {
    id: i64,
    product_id: i64,
    name: String,
    qty: i32,
    price: i32,
    line_total: i64,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/cart", get(api_cart_get))
        .route("/api/cart/items", post(api_cart_add_item))
        .route("/api/cart/items/:id", put(api_cart_update_item))
        .route("/api/cart/items/:id", delete(api_cart_delete_item))
}

/* ==================== handlers ==================== */

async fn api_cart_get(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    let (_, user) = get_user_from_cookie(&jar, &state).await;
    if user.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "login first").into_response();
    }
    let u = user.unwrap();
    let cart_id = ensure_viewer_cart(&state.db, u.id).await;

    let rows = sqlx::query!(
        r#"
        SELECT ci.id, ci.product_id, ci.qty, ci.price_at_add, p.name
        FROM cart_items ci JOIN products p ON ci.product_id=p.id
        WHERE ci.cart_id=?
        "#,
        cart_id
    )
    .fetch_all(&state.db)
    .await
    .unwrap();

    let mut items = vec![];
    let mut subtotal: i64 = 0;
    for r in rows {
        let line = r.qty as i64 * r.price_at_add as i64;
        subtotal += line;
        items.push(CartItemView {
            id: r.id,
            product_id: r.product_id,
            name: r.name,
            qty: r.qty,
            price: r.price_at_add,
            line_total: line,
        });
    }

    Json(CartView { cart_id, items, subtotal }).into_response()
}

async fn api_cart_add_item(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AddItemReq>,
) -> Response {
    let (_, user) = get_user_from_cookie(&jar, &state).await;
    if user.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "login first").into_response();
    }
    let u = user.unwrap();

    let prod = sqlx::query!(
        "SELECT id, price_idr FROM products WHERE id=? AND is_active=1",
        req.product_id
    )
    .fetch_optional(&state.db)
    .await
    .unwrap();
    if prod.is_none() {
        return (axum::http::StatusCode::BAD_REQUEST, "product missing").into_response();
    }
    let prod = prod.unwrap();

    let cart_id = ensure_viewer_cart(&state.db, u.id).await;

    if let Some(ci) = sqlx::query!(
        "SELECT id, qty FROM cart_items WHERE cart_id=? AND product_id=?",
        cart_id,
        prod.id
    )
    .fetch_optional(&state.db)
    .await
    .unwrap()
    {
        let _ = sqlx::query!("UPDATE cart_items SET qty=? WHERE id=?", ci.qty + req.qty, ci.id)
            .execute(&state.db)
            .await
            .unwrap();
    } else {
        let _ = sqlx::query!(
            "INSERT INTO cart_items(cart_id,product_id,qty,price_at_add) VALUES(?,?,?,?)",
            cart_id,
            prod.id,
            req.qty,
            prod.price_idr
        )
        .execute(&state.db)
        .await
        .unwrap();
    }

    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn api_cart_update_item(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(item_id): Path<i64>,
    Json(req): Json<UpdateItemReq>,
) -> Response {
    let (_, user) = get_user_from_cookie(&jar, &state).await;
    if user.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "login first").into_response();
    }
    let u = user.unwrap();

    let row = sqlx::query!(
        r#"
        SELECT ci.id FROM cart_items ci
        JOIN carts c ON ci.cart_id=c.id
        WHERE ci.id=? AND c.user_id=? AND c.status='open' LIMIT 1
        "#,
        item_id,
        u.id
    )
    .fetch_optional(&state.db)
    .await
    .unwrap();
    if row.is_none() {
        return (axum::http::StatusCode::FORBIDDEN, "no access").into_response();
    }

    if req.qty <= 0 {
        let _ = sqlx::query!("DELETE FROM cart_items WHERE id=?", item_id)
            .execute(&state.db)
            .await
            .unwrap();
    } else {
        let _ = sqlx::query!("UPDATE cart_items SET qty=? WHERE id=?", req.qty, item_id)
            .execute(&state.db)
            .await
            .unwrap();
    }

    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn api_cart_delete_item(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(item_id): Path<i64>,
) -> Response {
    let (_, user) = get_user_from_cookie(&jar, &state).await;
    if user.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "login first").into_response();
    }
    let u = user.unwrap();

    let row = sqlx::query!(
        r#"
        SELECT ci.id FROM cart_items ci
        JOIN carts c ON ci.cart_id=c.id
        WHERE ci.id=? AND c.user_id=? AND c.status='open' LIMIT 1
        "#,
        item_id,
        u.id
    )
    .fetch_optional(&state.db)
    .await
    .unwrap();
    if row.is_none() {
        return (axum::http::StatusCode::FORBIDDEN, "no access").into_response();
    }

    let _ = sqlx::query!("DELETE FROM cart_items WHERE id=?", item_id)
        .execute(&state.db)
        .await
        .unwrap();

    Json(serde_json::json!({ "ok": true })).into_response()
}

/* ================ helpers lokal ================ */

async fn ensure_viewer_cart(db: &Pool<MySql>, user_id: i64) -> i64 {
    if let Some(r) = sqlx::query!(
        "SELECT id FROM carts WHERE user_id=? AND status='open' LIMIT 1",
        user_id
    )
    .fetch_optional(db)
    .await
    .unwrap()
    {
        r.id
    } else {
        sqlx::query!("INSERT INTO carts(user_id,status) VALUES(?, 'open')", user_id)
            .execute(db)
            .await
            .unwrap()
            .last_insert_id() as i64
    }
}
