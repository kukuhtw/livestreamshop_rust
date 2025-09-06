// server/src/main.rs
/*
=============================================================================
Project : LiveStreamShop Rust — sell via live stream, chat & checkout on your site. 
Author  : Kukuh Tripamungkas Wicaksono (Kukuh TW)
Email   : kukuhtw@gmail.com
WhatsApp: https://wa.me/628129893706
LinkedIn: https://id.linkedin.com/in/kukuhtw
=============================================================================
*/


mod handlers; // server/src/handlers/
use handlers::{
    admin as admin_handlers,
    orders as orders_handlers,
    products as products_handlers,
    users as users_handlers,
    cart as cart_handlers,
};

use std::{collections::HashMap, env, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
     response::{Html, IntoResponse, Redirect},
    routing::get,
    Router,
};
use axum_extra::extract::cookie::CookieJar;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlPoolOptions, MySql, Pool};
use tokio::sync::{broadcast, RwLock};
use tower_http::services::ServeDir;



#[derive(Clone)]
pub(crate) struct AppState {
    pub rooms: Arc<RwLock<HashMap<String, broadcast::Sender<String>>>>,
    pub db: Pool<MySql>,
    pub cookie_name: String,
    // + NEW: kanal notifikasi global (order, dsb)
    pub notify_tx: broadcast::Sender<String>,
     pub viewer_counts: Arc<RwLock<HashMap<String, usize>>>, // NEW
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub(crate) struct User {
    pub id: i64,
    pub role: String, // "viewer" | "admin"
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "t")]
enum WsMsg {
    #[serde(rename = "f")]
    Frame { room: String, d: String },
    #[serde(rename = "c")]
    Chat { room: String, user: String, text: String },
    #[serde(rename = "sys")]
    Sys { room: String, text: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();


    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "mysql://user:password@127.0.0.1:3306/livestream_shop".into());

    let db = MySqlPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    // + NEW: buat channel global notifikasi
    let (notify_tx, _rx) = broadcast::channel::<String>(256);


    let state = AppState {
        rooms: Arc::new(RwLock::new(HashMap::new())),
        db,
        cookie_name: env::var("SESSION_COOKIE_NAME").unwrap_or_else(|_| "sid".into()),
        notify_tx, // + NEW
        viewer_counts: Arc::new(RwLock::new(HashMap::new())), // NEW
    };

    let app = Router::new()
        // pages & ws
        .route("/", get(index))
        .route("/live/:room", get(live_page))
        .route("/ws/:room", get(ws_handler))
        // merge modular handlers
        .merge(users_handlers::routes())
        .merge(admin_handlers::routes())
        .merge(products_handlers::routes())
        .merge(orders_handlers::routes())
        .merge(cart_handlers::routes())
        // static + uploads
        .nest_service("/static", ServeDir::new("../webapp"))
        .nest_service("/uploads", ServeDir::new("../uploads"))
        .with_state(state);

    let (listener, pretty_addr) = bind_with_fallback().await;
    println!("▶ serving at {pretty_addr}  (index di /static/index.html)");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn index() -> impl IntoResponse {
    Html(r#"<!doctype html><meta http-equiv="refresh" content="0; url=/static/index.html">Redirecting to /static/index.html..."#)
}

async fn live_page(Path(room): Path<String>) -> impl IntoResponse {
    // sanitize room: alnum, '_' atau '-'
    let safe: String = room
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();

    // arahkan ke file statis dengan query ?room=
    let target = format!("/static/livepage.html?room={}", safe);
    Redirect::temporary(&target)
}

/* ========= Helper umum (dipakai lintas modul) ========= */

pub(crate) async fn get_user_from_cookie(
    jar: &CookieJar,
    state: &AppState,
) -> (CookieJar, Option<User>) {
    if let Some(c) = jar.get(&state.cookie_name) {
        let sid = c.value().to_string();
        if let Some(row) = sqlx::query!(
            r#"
            SELECT u.id, u.role, u.name, u.email, u.phone
            FROM sessions s JOIN users u ON s.user_id=u.id
            WHERE s.sid = ? LIMIT 1
        "#,
            sid
        )
        .fetch_optional(&state.db)
        .await
        .unwrap()
        {
            return (
                jar.clone(),
                Some(User {
                    id: row.id,
                    role: row.role,
                    name: row.name,
                    email: row.email,
                    phone: row.phone,
                }),
            );
        }
    }
    (jar.clone(), None)
}

pub(crate) async fn ensure_admin(jar: &CookieJar, state: &AppState) -> Option<User> {
    let (_, u) = get_user_from_cookie(jar, state).await;
    if let Some(u) = u {
        if u.role == "admin" {
            return Some(u);
        }
    }
    None
}

/* ===================== WebSocket Streaming ===================== */

async fn ws_handler(
    State(state): State<AppState>,
    Path(room): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, room))
}

async fn handle_socket(socket: WebSocket, state: AppState, room: String) {
    // pilih channel room / global
    let tx = if room == "_events" {
        state.notify_tx.clone()
    } else {
        let mut rooms = state.rooms.write().await;
        rooms.entry(room.clone())
            .or_insert_with(|| broadcast::channel::<String>(512).0)
            .clone()
    };

    let mut rx = tx.subscribe();

    // === Viewer counter (untuk room selain _events)
    let is_view_room = room != "_events";
    if is_view_room {
        // increment
        {
            let mut vc = state.viewer_counts.write().await;
            let c = vc.entry(room.clone()).or_default();
            *c += 1;
        }
        // kirim notifikasi join + total
        let total_now = {
            let vc = state.viewer_counts.read().await;
            vc.values().copied().sum::<usize>()
        };
        let _ = state.notify_tx.send(serde_json::json!({"t":"viewer_join","room":room}).to_string());
        let _ = state.notify_tx.send(serde_json::json!({"t":"viewer_total","n":total_now}).to_string());
    }

    let _ = tx.send(
        serde_json::to_string(&WsMsg::Sys {
            room: room.clone(),
            text: "Client joined".into(),
        }).unwrap(),
    );

    let (mut writer_ws, mut reader_ws) = socket.split();

    let writer = tokio::spawn({
        let mut rx2 = rx;
        async move {
            while let Ok(msg) = rx2.recv().await {
                if writer_ws.send(Message::Text(msg)).await.is_err() {
                    break;
                }
            }
        }
    });

    // ===== reader loop
    while let Some(Ok(msg)) = reader_ws.next().await {
        match msg {
            Message::Text(txt) => {
                if txt.len() > 2_000_000 { continue; }
                if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(&txt) {
                    if parsed.get("room").is_none() {
                        parsed["room"] = serde_json::Value::String(room.clone());
                    }
                    let _ = tx.send(parsed.to_string());
                }
            }
            Message::Binary(_) => {}
            Message::Ping(_) | Message::Pong(_) => {}
            Message::Close(_) => break,
        }
    }

    // on close: decrement
    if is_view_room {
        {
            let mut vc = state.viewer_counts.write().await;
            if let Some(c) = vc.get_mut(&room) { if *c > 0 { *c -= 1; } }
        }
        let total_now = {
            let vc = state.viewer_counts.read().await;
            vc.values().copied().sum::<usize>()
        };
        let _ = state.notify_tx.send(serde_json::json!({"t":"viewer_total","n":total_now}).to_string());
    }

    writer.abort();
}
/* ===================== Bind helper ===================== */

async fn bind_with_fallback() -> (tokio::net::TcpListener, String) {
    // baca PORT dari env
    let port = std::env::var("PORT").unwrap_or_else(|_| "3030".to_string());
    let addr = format!("127.0.0.1:{}", port);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|_| panic!("❌ gagal bind ke port {}", port));

    let pretty = format!("http://{}/", listener.local_addr().unwrap());
    (listener, pretty)
}
