use anyhow::Result;
use futures::StreamExt;
use rustix::fs::{MemfdFlags, memfd_create, sendfile};
use std::{
    fs::File,
    io::{ErrorKind, Write},
    net::SocketAddr,
    sync::{
        LazyLock,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::{io::Interest, net::TcpListener, sync::Notify};
use tokio_tungstenite::tungstenite::Message;
use tracing::{Level, debug, info};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

static MEMFD: LazyLock<File> = LazyLock::new(|| {
    File::from(
        memfd_create(
            "jetrelay.dat",
            MemfdFlags::CLOEXEC | MemfdFlags::NOEXEC_SEAL,
        )
        .unwrap(),
    )
});
static FILE_LEN: AtomicU64 = AtomicU64::new(0);
static NOTIFY: Notify = Notify::const_new();

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Set up the logger
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

    // Write data from upstream into MEMFD
    tokio::spawn(async move {
        while let Some(msg) = upstream.next().await {
            let msg = msg?;
            match msg {
                Message::Ping(_) | Message::Pong(_) => (),
                Message::Text(msg) => {
                    let frame = text_frame(msg.as_str());
                    (&*MEMFD).write_all(&frame)?;
                    FILE_LEN.fetch_add(frame.len() as u64, Ordering::Release);
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
            let conn = ws.into_inner();
            // info!("Client connected");
            let mut offset = FILE_LEN.load(Ordering::Acquire);
            loop {
                NOTIFY.notified().await;
                // There's data to send - send it!
                let file_len = FILE_LEN.load(Ordering::Acquire);
                // let n = (file_len - offset) as usize;
                match conn
                    .async_io(Interest::WRITABLE, || {
                        rustix::fs::sendfile(&conn, &*MEMFD, Some(&mut offset), 2 << 20)
                            .map_err(|e| e.into())
                    })
                    .await
                {
                    Ok(_) => (),
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
