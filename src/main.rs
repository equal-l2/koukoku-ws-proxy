use std::net::ToSocketAddrs;
use std::sync::Arc;

use axum::extract::ws::{Message as WSMessage, WebSocket, WebSocketUpgrade};
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

#[cfg(feature = "serve-ui")]
use tower_http::services::ServeDir;

const LISTEN_ADDR: &str = "127.0.0.1";
const LISTEN_PORT: u16 = 8080;

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
            error!("Failed to connect with koukoku: {e:?}");
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
        info!("WebSocket closed with error: {e:?}");
    } else {
        info!("WebSocket closed");
    }
}

#[derive(miniserde::Serialize)]
enum PayloadKind {
    Houwa,
    Enzetsu,
}

#[derive(miniserde::Serialize)]
struct Message<'a> {
    kind: PayloadKind,
    payload: &'a str,
}

impl From<Message<'_>> for WSMessage {
    fn from(value: Message) -> Self {
        WSMessage::Text(miniserde::json::to_string(&value))
    }
}

/// 外から読んでWebSocketに流す
async fn relay_read<F>(ext_rx: impl AsyncRead + Unpin, mut ws_tx: F) -> anyhow::Result<()>
where
    F: futures::Sink<WSMessage> + Unpin,
    <F as futures::Sink<WSMessage>>::Error: Send + Sync + std::error::Error + 'static,
{
    // TODO: パーサの切り出し
    // TODO: 放話パース改善（色付き如何や自他で放話のフォーマットが微妙に違う　取りこぼしたエスケープや改行が演説に漏れている）

    trace!("relay_read");
    let mut reader = tokio::io::BufReader::new(ext_rx);

    //  複数行来る場合があるのでくっつけるためのやつ
    let mut line_buf = Vec::new();

    let mut buf = String::new();

    let mut in_houwa = false;

    // 初めて文字が来るときにやってくるヘッダ
    // BOM("\xef\xbb\xbf") + 書式リセット("\x1b[0m") + 改行文字
    // 改行文字部分はやや怪しい (CRLFのはずだが、CRCRLFだったことがある)
    let header = [0xef, 0xbb, 0xbf, 0x1b, 0x5b, 0x30, 0x6d];

    let is_start_houwa = |buf: &str| {
        // 演説内の">>"は前に半角スペースが入るので混同することはない
        // (演説および公告の各行は（一部の空行を除いて）半角スペースから始まる)
        const START_PATTERNS: &[&str] = &[
            ">>",                       // 通常
            "\x1b[0m\x1b[1m\x1b[31m>>", // 赤色開始
            "\x1b[0m\x1b[1m\x1b[32m>>", // 緑色開始(自分の放話)
        ];
        START_PATTERNS.iter().any(|s| buf.starts_with(s))
    };

    // let is_colored_end_houwa = |buf: &str| {
    //     // 色付き放話の終わり
    //     const START_PATTERNS: &[&[u8]] = &[
    //         b"\x1b[0m\x07\r\n", // 他者
    //         b"\x1b[0m\r\n",     // 自分（ベル文字がない）
    //     ];

    //     START_PATTERNS
    //         .iter()
    //         .any(|s| buf.bytes().eq(s.iter().copied()))
    // };

    macro_rules! send_houwa {
        ($payload: expr) => {{
            let msg = Message {
                kind: PayloadKind::Houwa,
                payload: $payload,
            };
            ws_tx.send(msg.into()).await
        }};
    }

    macro_rules! send_enzetsu {
        ($payload: expr) => {{
            let msg = Message {
                kind: PayloadKind::Enzetsu,
                payload: $payload,
            };
            ws_tx.send(msg.into()).await
        }};
    }

    loop {
        reader.read_line(&mut buf).await?;

        debug!("RECV Content: {buf}");
        debug!("RECV bytes: {:02x?}", buf.bytes().collect::<Vec<_>>());

        if buf.bytes().zip(header).all(|(x, y)| x == y) {
            // ヘッダは無視する
            debug!("ignore header");
        } else if is_start_houwa(&buf) && !in_houwa {
            if buf.ends_with("<<\x0d\x0a") {
                // （今のところ見たことないが）一行放話
                debug!("oneline houwa");
                send_houwa!(&buf)?;
            } else {
                // 放話開始
                debug!("start houwa");
                line_buf.push(buf.clone());
                in_houwa = true;
            }
        } else if in_houwa {
            line_buf.push(buf.clone());
            if buf.ends_with("<<\r\n") {
                // 放話終了
                debug!("end houwa");
                let joined = line_buf.join("").replace("\r\n", "");
                send_houwa!(&joined)?;
                line_buf.clear();
                in_houwa = false;
            }
        } else {
            // 放話以外は演説とみなす
            debug!("Send a enzetsu into WS");
            send_enzetsu!(&buf)?;
        }
        buf.clear()
    }
    // Ok(())
}

/// WebSocketから読んで外に流す
async fn relay_write<E>(
    mut ws_rx: impl futures::Stream<Item = Result<WSMessage, E>> + Unpin,
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
        if let WSMessage::Text(s) = msg {
            debug!("SEND Content: {s}");
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

    let app = Router::new().route("/ws", get(get_handler));

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
