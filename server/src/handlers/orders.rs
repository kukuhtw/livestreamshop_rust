// server/src/handlers/orders.rs
// server/src/handlers/orders.rs
use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{DateTime, Utc};
use rust_xlsxwriter::Workbook;
use serde::{Deserialize, Serialize};
use sqlx::{MySql, Pool};

use crate::{ensure_admin, get_user_from_cookie, AppState};

/* ===================== Input/Output types ===================== */

#[derive(Deserialize)]
pub struct CheckoutReq {
    pub shipping_name: String,
    pub shipping_phone: String,
    pub shipping_address: String,
    pub note: Option<String>,
    // tetap ada di request, tapi AKAN DIABAIKAN: viewer tidak boleh set ongkir
    pub delivery_fee: Option<i32>,
}

#[derive(Deserialize)]
pub struct AdminOrderPatch {
    pub delivery_fee: Option<i32>,
    pub total: Option<i32>,
    pub status: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
struct ItemOut {
    product_name: String,
    qty: i32,
    price_at_add: i32,
    line_total: i32,
}

#[derive(Serialize)]
struct AdminOrderOut {
    id: i64,
    user_id: i64,
    cart_id: i64,
    subtotal: i32,
    delivery_fee: i32,
    total: i32,
    shipping_name: String,
    shipping_phone: String,
    shipping_address: String,
    note: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
    items: Vec<ItemOut>, // items per order
}

// Detail order untuk viewer (atau admin membaca satu order)
#[derive(Serialize)]
struct OrderDetailOut {
    id: i64,
    cart_id: i64,
    subtotal: i32,
    delivery_fee: i32,
    total: i32,
    shipping_name: String,
    shipping_phone: String,
    shipping_address: String,
    note: Option<String>,
    status: String,
    created_at: DateTime<Utc>,
    items: Vec<ItemOut>,
}

/* ===================== Routes ===================== */

pub fn routes() -> Router<AppState> {
    Router::new()
        // Viewer
        .route("/api/orders", post(api_order_checkout))
        .route("/api/orders/:id", get(api_order_detail))
        .route("/api/orders/:id", delete(api_order_delete)) // NEW: delete (soft)
        .route("/api/orders/:id/export", get(api_order_export_xlsx)) // NEW: export xlsx
        // Admin
        .route("/api/admin/orders", get(api_admin_orders_list))
        .route("/api/admin/orders/:id", patch(api_admin_orders_patch))
        .route("/api/admin/orders/export", get(api_admin_orders_export_xlsx)) // NEW: export semua orders

}

/* ===================== Handlers ===================== */

pub async fn api_order_checkout(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CheckoutReq>,
) -> Response {
    // pastikan ada user dari cookie
    let (_, user) = get_user_from_cookie(&jar, &state).await;
    if user.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "login first").into_response();
    }
    let u = user.unwrap();

    // ambil/atau buat cart open
    let cart_id = ensure_viewer_cart(&state.db, u.id).await;

    // hitung subtotal dari item pada cart tsb
    let rows = sqlx::query!("SELECT qty, price_at_add FROM cart_items WHERE cart_id=?", cart_id)
        .fetch_all(&state.db)
        .await
        .unwrap();

    if rows.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, "cart empty").into_response();
    }

    let mut subtotal: i64 = 0;
    for r in rows {
        subtotal += (r.qty as i64) * (r.price_at_add as i64);
    }

    // Viewer TIDAK BOLEH mengatur ongkir → set 0 saat order dibuat.
    let delivery: i64 = 0;
    let total = subtotal + delivery;

    // lock cart → ordered, lalu buat order
    let _ = sqlx::query!("UPDATE carts SET status='ordered' WHERE id=?", cart_id)
        .execute(&state.db)
        .await
        .unwrap();

    let res = sqlx::query!(
        r#"
        INSERT INTO orders(
            user_id, cart_id, subtotal, delivery_fee, total,
            shipping_name, shipping_phone, shipping_address, note, status
        )
        VALUES(?,?,?,?,?,?,?,?,?,'new')
        "#,
        u.id,
        cart_id,
        subtotal as i32,
        delivery as i32,
        total as i32,
        req.shipping_name,
        req.shipping_phone,
        req.shipping_address,
        req.note
    )
    .execute(&state.db)
    .await
    .unwrap();

    let order_id = res.last_insert_id() as i64;

    // Broadcast event order baru (dipakai admin/viewer untuk auto-refresh)
    let _ = state
        .notify_tx
        .send(serde_json::json!({ "t": "order", "order_id": order_id }).to_string());

    Json(serde_json::json!({ "ok": true, "order_id": order_id, "total": total }))
        .into_response()
}

// Detail order untuk viewer (pemilik) atau admin
pub async fn api_order_detail(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(order_id): Path<i64>,
) -> Response {
    // siapa user-nya?
    let (_, user_opt) = get_user_from_cookie(&jar, &state).await;

    // ambil order
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, cart_id, subtotal, delivery_fee, total,
               shipping_name, shipping_phone, shipping_address, note, status, created_at
        FROM orders WHERE id=? LIMIT 1
        "#,
        order_id
    )
    .fetch_optional(&state.db)
    .await
    .unwrap();

    if row.is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    }
    let row = row.unwrap();

    // otorisasi: pemilik order atau admin
    let is_admin = ensure_admin(&jar, &state).await.is_some();
    let is_owner = user_opt.as_ref().map(|u| u.id == row.user_id).unwrap_or(false);
    if !is_admin && !is_owner {
        return (axum::http::StatusCode::FORBIDDEN, "forbidden").into_response();
    }

    // ambil items
    let item_rows = sqlx::query!(
        r#"
        SELECT p.name AS product_name, ci.qty, ci.price_at_add
        FROM cart_items ci
        JOIN products p ON p.id = ci.product_id
        WHERE ci.cart_id = ?
        ORDER BY ci.id ASC
        "#,
        row.cart_id
    )
    .fetch_all(&state.db)
    .await
    .unwrap();

    let items: Vec<ItemOut> = item_rows
        .into_iter()
        .map(|it| {
            let qty = it.qty;
            let price = it.price_at_add;
            ItemOut {
                product_name: it.product_name, // kolom NOT NULL
                qty,
                price_at_add: price,
                line_total: qty * price,
            }
        })
        .collect();

    let out = OrderDetailOut {
        id: row.id,
        cart_id: row.cart_id,
        subtotal: row.subtotal,
        delivery_fee: row.delivery_fee,
        total: row.total,
        shipping_name: row.shipping_name,
        shipping_phone: row.shipping_phone,
        shipping_address: row.shipping_address,
        note: row.note,
        status: row.status,
        created_at: row.created_at,
        items,
    };

    Json(out).into_response()
}

// Admin: list seluruh order + items
pub async fn api_admin_orders_list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    if ensure_admin(&jar, &state).await.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "admin only").into_response();
    }

    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, cart_id, subtotal, delivery_fee, total,
               shipping_name, shipping_phone, shipping_address, note, status, created_at
        FROM orders
         WHERE status <> 'deleted'
        ORDER BY id DESC
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap();

    let mut out: Vec<AdminOrderOut> = Vec::with_capacity(rows.len());

    for r in rows {
        let item_rows = sqlx::query!(
            r#"
            SELECT p.name AS product_name, ci.qty, ci.price_at_add
            FROM cart_items ci
            JOIN products p ON p.id = ci.product_id
            WHERE ci.cart_id = ?
            ORDER BY ci.id ASC
            "#,
            r.cart_id
        )
        .fetch_all(&state.db)
        .await
        .unwrap();

        let items: Vec<ItemOut> = item_rows
            .into_iter()
            .map(|it| ItemOut {
                product_name: it.product_name,
                qty: it.qty,
                price_at_add: it.price_at_add,
                line_total: it.qty * it.price_at_add,
            })
            .collect();

        out.push(AdminOrderOut {
            id: r.id,
            user_id: r.user_id,
            cart_id: r.cart_id,
            subtotal: r.subtotal,
            delivery_fee: r.delivery_fee,
            total: r.total,
            shipping_name: r.shipping_name,
            shipping_phone: r.shipping_phone,
            shipping_address: r.shipping_address,
            note: r.note,
            status: r.status,
            created_at: r.created_at,
            items,
        });
    }

    Json(out).into_response()
}

// Admin: patch (update ongkir/total/status)
pub async fn api_admin_orders_patch(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(order_id): Path<i64>,
    Json(inp): Json<AdminOrderPatch>,
) -> Response {
    if ensure_admin(&jar, &state).await.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "admin only").into_response();
    }

    let ord = sqlx::query!("SELECT id, subtotal, delivery_fee, total FROM orders WHERE id=?", order_id)
        .fetch_optional(&state.db)
        .await
        .unwrap();

    if ord.is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    }
    let ord = ord.unwrap();

    // update delivery dulu (pakai nilai baru jika ada)
    let delivery = inp.delivery_fee.unwrap_or(ord.delivery_fee);

    // jika total diinput, pakai; jika tidak, hitung dari subtotal + delivery
    let total = inp.total.unwrap_or(ord.subtotal + delivery);

    let status = inp.status.unwrap_or_else(|| "new".into());

    let _ = sqlx::query!(
        "UPDATE orders SET delivery_fee=?, total=?, status=? WHERE id=?",
        delivery,
        total,
        status,
        order_id
    )
    .execute(&state.db)
    .await
    .unwrap();

    // Broadcast event update order (agar viewer reload detail)
    let _ = state
        .notify_tx
        .send(serde_json::json!({ "t": "order_update", "order_id": order_id }).to_string());

    Json(serde_json::json!({ "ok": true })).into_response()
}

/* ===================== Extra: Export XLSX & Delete ===================== */

// EXPORT: detail order ke .xlsx (attachment)
// EXPORT: detail order ke .xlsx (attachment)
pub async fn api_order_export_xlsx(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(order_id): Path<i64>,
) -> Response {
    // Ambil order + otorisasi owner/admin
    let (_, user_opt) = get_user_from_cookie(&jar, &state).await;

    let row = sqlx::query!(
        r#"
        SELECT id, user_id, cart_id, subtotal, delivery_fee, total,
               shipping_name, shipping_phone, shipping_address, note, status, created_at
        FROM orders WHERE id=? LIMIT 1
        "#,
        order_id
    )
    .fetch_optional(&state.db)
    .await
    .unwrap();

    if row.is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    }
    let row = row.unwrap();

    let is_admin = ensure_admin(&jar, &state).await.is_some();
    let is_owner = user_opt.as_ref().map(|u| u.id == row.user_id).unwrap_or(false);
    if !is_admin && !is_owner {
        return (axum::http::StatusCode::FORBIDDEN, "forbidden").into_response();
    }

    let items = sqlx::query!(
        r#"
        SELECT p.name AS product_name, ci.qty, ci.price_at_add
        FROM cart_items ci
        JOIN products p ON p.id = ci.product_id
        WHERE ci.cart_id = ?
        ORDER BY ci.id ASC
        "#,
        row.cart_id
    )
    .fetch_all(&state.db)
    .await
    .unwrap();

    // Build workbook (in-memory)
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();// Setelah ws dibuat
ws.set_column_width(0, 40).ok();
ws.set_column_width(1, 10).ok();
ws.set_column_width(2, 16).ok();
ws.set_column_width(3, 16).ok();


    // Header Order
    ws.write(0, 0, format!("Order #{}", row.id)).ok();
    ws.write(1, 0, "Status").ok();            ws.write(1, 1, &row.status).ok();
    ws.write(2, 0, "Tanggal").ok();           ws.write(2, 1, row.created_at.to_rfc3339()).ok();
    ws.write(3, 0, "Penerima").ok();          ws.write(3, 1, &row.shipping_name).ok();
    ws.write(4, 0, "WhatsApp").ok();          ws.write(4, 1, &row.shipping_phone).ok();
    ws.write(5, 0, "Alamat").ok();            ws.write(5, 1, &row.shipping_address).ok();
    if let Some(note) = &row.note { ws.write(6, 0, "Catatan").ok(); ws.write(6, 1, note).ok(); }

    // Tabel Items (mulai baris 8)
    let start = 8u32;
    ws.write(start, 0, "Item").ok();
    ws.write(start, 1, "Qty").ok();
    ws.write(start, 2, "Harga (IDR)").ok();
    ws.write(start, 3, "Total (IDR)").ok();

    let mut r = start + 1;
    for it in items {
        let line_total = it.qty * it.price_at_add;
        ws.write(r, 0, it.product_name).ok();
        ws.write_number(r, 1, it.qty as f64).ok();
        ws.write_number(r, 2, it.price_at_add as f64).ok();
        ws.write_number(r, 3, line_total as f64).ok();
        r += 1;
    }

    // Ringkasan
    r += 1;
    ws.write(r, 2, "Subtotal").ok();     ws.write_number(r, 3, row.subtotal as f64).ok(); r += 1;
    ws.write(r, 2, "Ongkir").ok();       ws.write_number(r, 3, row.delivery_fee as f64).ok(); r += 1;
    ws.write(r, 2, "Grand Total").ok();  ws.write_number(r, 3, row.total as f64).ok();



    // ✅ Serialize ke buffer (API benar: tanpa argumen, mengembalikan Vec<u8>)
    let buf = match wb.save_to_buffer() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("xlsx export error: {e:?}");
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "export error").into_response();
        }
    };

    // Response: attachment
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
    );
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"order-{}.xlsx\"", row.id)).unwrap(),
    );

    (headers, buf).into_response()
}

// EXPORT SEMUA ORDERS (Admin-only): attachment XLSX
// EXPORT SEMUA ORDERS (Admin-only): attachment XLSX
pub async fn api_admin_orders_export_xlsx(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Response {
    // Wajib admin
    if ensure_admin(&jar, &state).await.is_none() {
        return (axum::http::StatusCode::UNAUTHORIZED, "admin only").into_response();
    }

    // Ambil semua order (terbaru dulu)
    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, cart_id, subtotal, delivery_fee, total,
               shipping_name, shipping_phone, shipping_address, note, status, created_at
        FROM orders
         WHERE status <> 'deleted'
        ORDER BY id DESC
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap();

    // Siapkan workbook
    let mut wb = rust_xlsxwriter::Workbook::new();
    let ws = wb.add_worksheet();

    // Header kolom
    ws.write(0, 0, "ID").ok();
    ws.write(0, 1, "Tanggal").ok();
    ws.write(0, 2, "Customer").ok();
    ws.write(0, 3, "Phone").ok();
    ws.write(0, 4, "Alamat").ok();
    ws.write(0, 5, "Status").ok();
    ws.write(0, 6, "Subtotal").ok();
    ws.write(0, 7, "Ongkir").ok();
    ws.write(0, 8, "Total").ok();
    ws.write(0, 9, "Items").ok();
    ws.write(0, 10, "Catatan").ok();

    // Lebar kolom biar rapi
    ws.set_column_width(0, 8).ok();   // ID
    ws.set_column_width(1, 22).ok();  // Tanggal
    ws.set_column_width(2, 20).ok();  // Customer
    ws.set_column_width(3, 16).ok();  // Phone
    ws.set_column_width(4, 36).ok();  // Alamat
    ws.set_column_width(5, 12).ok();  // Status
    ws.set_column_width(6, 14).ok();  // Subtotal
    ws.set_column_width(7, 12).ok();  // Ongkir
    ws.set_column_width(8, 14).ok();  // Total
    ws.set_column_width(9, 48).ok();  // Items
    ws.set_column_width(10, 28).ok(); // Catatan

    // Isi baris
    let mut r: u32 = 1;
    for o in rows {
        // Ambil items per order
        let item_rows = sqlx::query!(
            r#"
            SELECT p.name AS product_name, ci.qty, ci.price_at_add
            FROM cart_items ci
            JOIN products p ON p.id = ci.product_id
            WHERE ci.cart_id = ?
            ORDER BY ci.id ASC
            "#,
            o.cart_id
        )
        .fetch_all(&state.db)
        .await
        .unwrap();

        // Gabungkan items menjadi satu string
        let items_joined = if item_rows.is_empty() {
            String::from("-")
        } else {
            item_rows
                .into_iter()
                .map(|it| {
                    let line_total = (it.qty * it.price_at_add) as i64;
                    format!(
                        "{} x {} @ {} = {}",
                        it.product_name,
                        it.qty,
                        it.price_at_add,
                        line_total
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ")
        };

        // created_at sudah DateTime<Utc> → langsung to_rfc3339()
        let date_str = o.created_at.to_rfc3339();

        ws.write_number(r, 0, o.id as f64).ok();
        ws.write(r, 1, date_str).ok();
        ws.write(r, 2, &o.shipping_name).ok();
        ws.write(r, 3, &o.shipping_phone).ok();
        ws.write(r, 4, &o.shipping_address).ok();
        ws.write(r, 5, &o.status).ok();
        ws.write_number(r, 6, o.subtotal as f64).ok();
        ws.write_number(r, 7, o.delivery_fee as f64).ok();
        ws.write_number(r, 8, o.total as f64).ok();
        ws.write(r, 9, items_joined).ok();
        if let Some(note) = &o.note { ws.write(r, 10, note).ok(); }
        r += 1;
    }

    // Simpan ke buffer
    let buf = match wb.save_to_buffer() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("export all orders XLSX error: {e:?}");
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "export error").into_response();
        }
    };

    // Attachment headers
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    );
    headers.insert(
        axum::http::header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"orders-all.xlsx\""),
    );
    (headers, buf).into_response()
}

// DELETE: soft-delete order (status='deleted')
pub async fn api_order_delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(order_id): Path<i64>,
) -> Response {
    // Ambil order + cek otorisasi owner/admin
    let (_, user_opt) = get_user_from_cookie(&jar, &state).await;

    let row = sqlx::query!("SELECT id, user_id, status FROM orders WHERE id=? LIMIT 1", order_id)
        .fetch_optional(&state.db)
        .await
        .unwrap();

    if row.is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    }
    let row = row.unwrap();

    let is_admin = ensure_admin(&jar, &state).await.is_some();
    let is_owner = user_opt.as_ref().map(|u| u.id == row.user_id).unwrap_or(false);
    if !is_admin && !is_owner {
        return (axum::http::StatusCode::FORBIDDEN, "forbidden").into_response();
    }

    // (Opsional) batasi hanya status tertentu
    // if row.status != "new" {
    //     return (axum::http::StatusCode::BAD_REQUEST, "only 'new' deletable").into_response();
    // }

    let _ = sqlx::query!("UPDATE orders SET status='deleted' WHERE id=?", order_id)
        .execute(&state.db)
        .await
        .unwrap();

    // Broadcast
    let _ = state
        .notify_tx
        .send(serde_json::json!({ "t": "order_deleted", "order_id": order_id }).to_string());

    Json(serde_json::json!({ "ok": true })).into_response()
}

/* ===================== Local helpers (khusus modul ini) ===================== */

async fn ensure_viewer_cart(db: &Pool<MySql>, user_id: i64) -> i64 {
    if let Some(r) =
        sqlx::query!("SELECT id FROM carts WHERE user_id=? AND status='open' LIMIT 1", user_id)
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
