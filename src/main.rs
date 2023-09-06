use std::net::ToSocketAddrs;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use tokio::{
    self,
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt},
};

const LISTEN_ADDR: &str = "127.0.0.1";
const LISTEN_PORT: u16 = 8080;

const PROXY_ENDPOINT: &str = "/ws";

const SERVER_HOST: &str = "koukoku.shadan.open.ad.jp";
const SERVER_PORT: u16 = 992;

fn prepare_tls_connector() -> tokio_rustls::TlsConnector {
    let mut root_store = tokio_rustls::rustls::RootCertStore::empty();
    root_store.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|ta| {
        tokio_rustls::rustls::OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));

    let config = tokio_rustls::rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    tokio_rustls::TlsConnector::from(Arc::new(config))
}

async fn connect_koukoku() -> anyhow::Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>
{
    let stream = tokio::net::TcpStream::connect((SERVER_HOST, SERVER_PORT)).await?;

    let conn = prepare_tls_connector();
    let domain = tokio_rustls::rustls::ServerName::try_from(SERVER_HOST)?;
    let stream = conn.connect(domain, stream).await?;

    Ok(stream)
}

async fn ws_handler(sock: WebSocket) {
    let stream = match connect_koukoku().await {
        Ok(inner) => inner,
        Err(e) => {
            eprintln!("INFO: Failed to connect with koukoku: {:?}", e);
            return;
        }
    };
    eprintln!("INFO: Connected to koukoku");

    let (ext_rx, ext_tx) = tokio::io::split(stream);

    let (ws_tx, ws_rx) = sock.split();

    let res = tokio::try_join!(connect_read(ext_rx, ws_tx), connect_write(ws_rx, ext_tx));
    if let Err(e) = res {
        eprintln!("INFO: WebSocket closed: {:?}", e);
    }
}

/// 外から読んでWebSocketに流す
async fn connect_read<F>(ext_rx: impl AsyncRead + Unpin, mut ws_tx: F) -> anyhow::Result<()>
where
    F: futures::Sink<Message> + Unpin,
    <F as futures::Sink<axum::extract::ws::Message>>::Error:
        Send + Sync + std::error::Error + 'static,
{
    let mut reader = tokio::io::BufReader::new(ext_rx);
    let mut buf = String::new();
    loop {
        reader.read_line(&mut buf).await?;

        // チャットメッセージのみを流す
        if buf.starts_with(">>") {
            ws_tx.send(Message::Text(buf.clone())).await?;
        }
        buf.clear()
    }
}

/// WebSocketから読んで外に流す
async fn connect_write<E>(
    mut ws_rx: impl futures::Stream<Item = Result<Message, E>> + Unpin,
    mut ext_tx: impl AsyncWrite + Unpin,
) -> anyhow::Result<()>
where
    E: Send + Sync + std::error::Error + 'static,
{
    while let Some(msg) = ws_rx.next().await {
        let msg = msg?;
        if let Message::Text(s) = msg {
            ext_tx.write_all(s.as_bytes()).await?;
        }
    }

    Ok(())
}

async fn get_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(ws_handler)
}

#[tokio::main]
async fn main() {
    let app = Router::new().route(PROXY_ENDPOINT, get(get_handler));

    let addr = (LISTEN_ADDR, LISTEN_PORT)
        .to_socket_addrs()
        .expect("invalid address configuration")
        .next()
        .expect("should contain an entry");

    println!("Listening on port {}", LISTEN_PORT);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap()
}
