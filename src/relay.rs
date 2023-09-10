use std::sync::Arc;

use axum::extract::ws::{Message as WSMessage, WebSocket};
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use log::*;
use once_cell::sync::Lazy;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt};

static TLS_CONNECTOR: Lazy<tokio_rustls::TlsConnector> = once_cell::sync::Lazy::new(|| {
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
});

async fn connect_koukoku() -> anyhow::Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>
{
    const SERVER_HOST: &str = "koukoku.shadan.open.ad.jp";
    const SERVER_PORT: u16 = 992;

    let stream = tokio::net::TcpStream::connect((SERVER_HOST, SERVER_PORT)).await?;

    let domain = tokio_rustls::rustls::ServerName::try_from(SERVER_HOST)?;
    let stream = TLS_CONNECTOR.connect(domain, stream).await?;

    Ok(stream)
}

pub async fn handler(sock: WebSocket) {
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
struct Message {
    kind: PayloadKind,
    payload: String,
}

impl From<Message> for WSMessage {
    fn from(value: Message) -> Self {
        WSMessage::Text(miniserde::json::to_string(&value))
    }
}

struct Parser {
    // 複数行放話をくっつけるためのバッファ
    line_buf: Vec<String>,
    // 放話のパース中かどうか
    in_houwa: bool,
}

impl Parser {
    fn new() -> Self {
        Self {
            line_buf: vec![],
            in_houwa: false,
        }
    }

    fn is_start_of_houwa(target: &str) -> bool {
        // 演説内の">>"は前に半角スペースが入るので混同することはない
        // (演説および公告の各行は（一部の空行を除いて）半角スペースから始まる)
        const START_PATTERNS: &[&str] = &[
            ">>",                       // 通常
            "\x1b[0m\x1b[1m\x1b[31m>>", // 赤色開始
            "\x1b[0m\x1b[1m\x1b[32m>>", // 緑色開始(自分の放話)
        ];
        START_PATTERNS.iter().any(|s| target.starts_with(s))
    }

    fn parse(&mut self, buf: &str) -> Option<Message> {
        // TODO: 放話パース改善（色付き如何や自他で放話のフォーマットが微妙に違う　取りこぼしたエスケープや改行が演説に漏れている）

        // 初めて文字が来るときにやってくるヘッダ
        // BOM("\xef\xbb\xbf") + 書式リセット("\x1b[0m") + 改行文字
        // 改行文字部分はやや怪しい (CRLFのはずだが、CRCRLFだったことがある)
        const HEADER: &[u8] = &[0xef, 0xbb, 0xbf, 0x1b, 0x5b, 0x30, 0x6d];

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

        if buf.bytes().zip(HEADER).all(|(x, y)| &x == y) {
            // ヘッダは無視する
            debug!("ignore header");
            None
        } else if Self::is_start_of_houwa(buf) && !self.in_houwa {
            if buf.ends_with("<<\x0d\x0a") {
                // （今のところ見たことないが）一行放話
                debug!("oneline houwa");
                Some(Message {
                    kind: PayloadKind::Houwa,
                    payload: buf.to_owned(),
                })
            } else {
                // 放話開始
                debug!("start houwa");
                self.line_buf.push(buf.to_owned());
                self.in_houwa = true;
                None
            }
        } else if self.in_houwa {
            self.line_buf.push(buf.to_owned());
            if buf.ends_with("<<\r\n") {
                // 放話終了
                debug!("end houwa");
                let joined = self.line_buf.join("").replace("\r\n", "");
                self.line_buf.clear();
                self.in_houwa = false;
                Some(Message {
                    kind: PayloadKind::Houwa,
                    payload: joined,
                })
            } else {
                None
            }
        } else {
            // 放話以外は演説とみなす
            debug!("Send a enzetsu into WS");
            Some(Message {
                kind: PayloadKind::Enzetsu,
                payload: buf.to_owned(),
            })
        }
    }
}

/// 外から読んでWebSocketに流す
async fn relay_read<F>(ext_rx: impl AsyncRead + Unpin, mut ws_tx: F) -> anyhow::Result<()>
where
    F: futures::Sink<WSMessage> + Unpin,
    <F as futures::Sink<WSMessage>>::Error: Send + Sync + std::error::Error + 'static,
{
    trace!("relay_read");
    let mut reader = tokio::io::BufReader::new(ext_rx);

    let mut buf = String::new();
    let mut parser = Parser::new();

    loop {
        reader.read_line(&mut buf).await?;

        debug!("RECV Content: {buf}");
        debug!("RECV bytes: {:02x?}", buf.bytes().collect::<Vec<_>>());

        if let Some(msg) = parser.parse(&buf) {
            ws_tx.send(msg.into()).await?;
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
