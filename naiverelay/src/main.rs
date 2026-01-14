use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::broadcast::{self, Receiver},
};
use tokio_tungstenite::tungstenite::{Message, Utf8Bytes};
use tracing::{Level, error, info, warn};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

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
    let port: u16 = std::env::var(var).context(var)?.parse().context(var)?;
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr).await?;
    info!(%listen_addr, "Bound listener");

    // Connect to upstream
    let var = "UPSTREAM_URL";
    let upstream_url = std::env::var(var).context(var)?;
    let (mut upstream, _) = tokio_tungstenite::connect_async(&upstream_url).await?;
    info!(%upstream_url, "Connected to upstream");

    let (tx, rx) = broadcast::channel::<Utf8Bytes>(1024);

    let total_bytes = Arc::new(AtomicUsize::new(0));
    let total_bytes_2 = total_bytes.clone();
    tokio::spawn(async move {
        while let Some(msg) = upstream.next().await {
            let msg = msg?;
            total_bytes_2.fetch_add(msg.len(), Ordering::Release);
            match msg {
                Message::Ping(_) | Message::Pong(_) => (),
                Message::Text(msg) => {
                    tx.send(msg)?;
                }
                _ => panic!("{msg:?}"),
            }
        }
        anyhow::Ok(())
    });

    tokio::spawn(async move {
        loop {
            info!(
                "{:.2} Mbps",
                total_bytes.load(Ordering::Acquire) as f32 * 8. / 1024. / 1024.
            );
            total_bytes.store(0, Ordering::Release);
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    while let Ok((stream, _)) = listener.accept().await {
        let rx = rx.resubscribe();
        tokio::spawn(async move {
            match handle_client(stream, rx).await {
                Ok(()) => (),
                Err(e) => error!("{e:#}"),
            }
        });
    }

    Ok(())
}

async fn handle_client(stream: TcpStream, mut rx: Receiver<Utf8Bytes>) -> Result<()> {
    let mut ws = tokio_tungstenite::accept_async(stream).await?;
    info!("Client connected");
    loop {
        match rx.recv().await {
            Ok(msg) => ws.send(Message::Text(msg)).await?,
            Err(broadcast::error::RecvError::Closed) => {
                error!("Shutting down");
                return anyhow::Ok(());
            }
            Err(broadcast::error::RecvError::Lagged(x)) => {
                warn!("Lagged: {x}");
            }
        }
    }
}
