use std::net::ToSocketAddrs;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use log::*;
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
    info!("new Websocket connection accepted");
    let stream = match connect_koukoku().await {
        Ok(inner) => inner,
        Err(e) => {
            error!("Failed to connect with koukoku: {:?}", e);
            return;
        }
    };

    info!("Connected to koukoku");

    let (ext_rx, ext_tx) = tokio::io::split(stream);

    let (ws_tx, ws_rx) = sock.split();

    let read_handle = relay_read(ext_rx, ws_tx);
    let write_handle = relay_write(ws_rx, ext_tx);

    // 片方が終了したら終わる
    let res = tokio::select! {
        val = read_handle => {
            val
        }
        val = write_handle => {
            val
        }
    };

    if let Err(e) = res {
        info!("WebSocket closed with error: {:?}", e);
    } else {
        info!("WebSocket closed");
    }
}

/// 外から読んでWebSocketに流す
async fn relay_read<F>(ext_rx: impl AsyncRead + Unpin, mut ws_tx: F) -> anyhow::Result<()>
where
    F: futures::Sink<Message> + Unpin,
    <F as futures::Sink<axum::extract::ws::Message>>::Error:
        Send + Sync + std::error::Error + 'static,
{
    trace!("relay_read");
    let mut reader = tokio::io::BufReader::new(ext_rx);

    //  複数行来る場合があるのでくっつけるためのやつ
    let mut line_buf = Vec::new();

    let mut buf = String::new();

    let mut in_msg = false;
    loop {
        reader.read_line(&mut buf).await?;

        // チャットメッセージのみを流す
        // TODO: 演説
        trace!("RECV Content: {}", buf);
        trace!(
            "RECV bytes (first 5): {:?}",
            buf.bytes().collect::<Vec<_>>()
        );

        let is_start_msg = || {
            const START_PATTERNS: &[&str] = &[
                ">>",                       // 通常
                "\x1b[0m\x1b[1m\x1b[31m>>", // 赤色開始
                "\x1b[0m\x1b[1m\x1b[32m>>", // 緑色開始(自分の放話)
            ];
            START_PATTERNS.iter().any(|s| buf.starts_with(s))
        };
        let is_empty_end = || {
            const END_PATTERNS: &[&str] = &[
                "\r\n",            // 通常
                "\x1b[0m\x07\r\n", // 色付き終了
            ];
            END_PATTERNS.iter().any(|s| &buf == s)
        };
        if is_start_msg() && !in_msg {
            debug!("start message");
            line_buf.push(buf.clone());
            in_msg = true;
        } else if in_msg {
            line_buf.push(buf.clone());
            if buf.ends_with("<<\r\n") {
                debug!("end message");
                in_msg = false;
            } else if is_empty_end() {
                // なにかうまくいってないとき
                // 放話に空行は入らないはずなので終わらせる
                debug!("abrupt end message");
                in_msg = false;
            }

            if !in_msg {
                // 放話終了
                let msg = line_buf.join("").replace("\r\n", "");
                ws_tx.send(Message::Text(msg)).await?;
                line_buf.clear();
            }
        }
        buf.clear()
    }
}

/// WebSocketから読んで外に流す
async fn relay_write<E>(
    mut ws_rx: impl futures::Stream<Item = Result<Message, E>> + Unpin,
    mut ext_tx: impl AsyncWrite + Unpin,
) -> anyhow::Result<()>
where
    E: Send + Sync + std::error::Error + 'static,
{
    trace!("relay_write");
    // 公告表示無効化
    ext_tx.write_all(b"nobody\n").await?;
    while let Some(msg) = ws_rx.next().await {
        let msg = msg?;
        if let Message::Text(s) = msg {
            debug!("SEND Content: {}", s);
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
    pretty_env_logger::init();
    let app = Router::new().route(PROXY_ENDPOINT, get(get_handler));

    let addr = (LISTEN_ADDR, LISTEN_PORT)
        .to_socket_addrs()
        .expect("invalid address configuration")
        .next()
        .expect("should contain an entry");

    eprintln!("Listening on port {}", LISTEN_PORT);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap()
}
