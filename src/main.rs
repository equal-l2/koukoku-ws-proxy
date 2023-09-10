use std::net::ToSocketAddrs;

use axum::extract::ws::WebSocketUpgrade;
use axum::response::Response;
use axum::routing::get;
use axum::Router;

mod relay;

#[cfg(feature = "serve-ui")]
use tower_http::services::ServeDir;

const LISTEN_ADDR: &str = "127.0.0.1";
const LISTEN_PORT: u16 = 8080;

async fn handle_get(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(relay::handler)
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let app = Router::new().route("/ws", get(handle_get));

    #[cfg(feature = "serve-ui")]
    let app = app.nest_service("/ui", ServeDir::new("./static"));

    let addr = (LISTEN_ADDR, LISTEN_PORT)
        .to_socket_addrs()
        .expect("invalid address configuration")
        .next()
        .expect("should contain an entry");

    eprintln!("Listening on port {LISTEN_PORT}\n");

    eprintln!("WebSocket endpoint is available on `/ws`");
    eprintln!("( Typically in form of http://localhost:{LISTEN_PORT}/ws )\n");

    #[cfg(feature = "serve-ui")]
    {
        eprintln!("Web UI is available on `/ui`");
        eprintln!("( Typically in form of http://localhost:{LISTEN_PORT}/ui )");
    }

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap()
}
