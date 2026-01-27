use anyhow::Result;
use futures::{StreamExt, lock::Mutex};
use std::{io::ErrorKind, net::SocketAddr, sync::LazyLock};
use tokio::{io::AsyncWriteExt, net::TcpListener, sync::Notify};
use tokio_tungstenite::tungstenite::Message;
use tracing::{Level, debug, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

static DATA: LazyLock<Mutex<Vec<u8>>> = LazyLock::new(|| Mutex::new(vec![]));
static NOTIFY: Notify = Notify::const_new();

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();
    let writer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    tracing_subscriber::registry()
        .with(filter)
        .with(writer)
        .init();

    // Bind the listener socket
    let var = "JETRELAY_PORT";
    let port: u16 = match std::env::var(var) {
        Ok(x) => x.parse()?,
        Err(_) => 7375,
    };
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr).await?;
    info!(%listen_addr, "Bound listener");

    // Connect to upstream
    let var = "UPSTREAM_URL";
    let upstream_url = match std::env::var(var) {
        Ok(x) => x,
        Err(_) => "ws://localhost:7376/subscribe".to_string(),
    };
    let (mut upstream, _) = tokio_tungstenite::connect_async(&upstream_url).await?;
    info!(%upstream_url, "Connected to upstream");

    // Write data from upstream into DATA
    tokio::spawn(async move {
        while let Some(msg) = upstream.next().await {
            let msg = msg?;
            match msg {
                Message::Ping(_) | Message::Pong(_) => (),
                Message::Text(msg) => {
                    let frame = text_frame(msg.as_str());
                    DATA.lock().await.extend(&frame);
                    NOTIFY.notify_waiters();
                }
                _ => panic!("{msg:?}"),
            }
        }
        anyhow::Ok(())
    });

    while let Ok((conn, _)) = listener.accept().await {
        tokio::spawn(async move {
            // Do the websockets handshake
            let ws = tokio_tungstenite::accept_async(conn).await?;
            let mut conn = ws.into_inner();
            // info!("Client connected");
            let mut offset = DATA.lock().await.len();
            loop {
                NOTIFY.notified().await;
                let slice = &DATA.lock().await[offset..];
                match conn.write(slice).await {
                    Ok(n) => offset += n,
                    Err(e) => match e.kind() {
                        ErrorKind::BrokenPipe | ErrorKind::ConnectionReset => {
                            debug!("Socket closed by other side");
                            break anyhow::Ok(());
                        }
                        _ => Err(e)?,
                    },
                }
            }
        });
    }

    Ok(())
}

fn text_frame(payload: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(0b1000_0001); // text
    match payload.len() {
        ..126 => buf.push(payload.len() as u8),
        126..65535 => {
            buf.push(126);
            buf.extend((payload.len() as u16).to_be_bytes());
        }
        65535.. => {
            buf.push(127);
            buf.extend((payload.len() as u64).to_be_bytes());
        }
    }
    buf.extend_from_slice(payload.as_bytes());
    buf
}
