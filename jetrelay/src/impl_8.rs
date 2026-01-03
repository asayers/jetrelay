use crate::upstream::connect_to_upstream;
use anyhow::{Context, Result};
use std::net::SocketAddr;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};
use tracing::*;
use wsclient::Frame;

pub async fn run() -> Result<()> {
    // Bind the listener socket ASAP
    let var = "JETRELAY_PORT";
    let port: u16 = std::env::var(var).context(var)?.parse().context(var)?;
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr).await?;
    info!(%listen_addr, "Bound socket");

    let (tx, rx) = tokio::sync::broadcast::channel::<Frame>(1024);

    // Handle incoming client connections in a separate thread
    tokio::task::spawn(async move {
        loop {
            let (conn, _) = listener.accept().await.unwrap();
            let mut rx = rx.resubscribe();
            tokio::task::spawn(async move {
                let conn = conn.into_std()?;
                let mut conn2 = conn.try_clone()?;
                let mut conn = TcpStream::from_std(conn)?;
                let _config = tokio::task::spawn_blocking(move || {
                    crate::handshake::perform_handshake(&mut conn2)
                })
                .await
                .unwrap()
                .unwrap();
                tokio::task::spawn(async move {
                    loop {
                        let frame = rx.recv().await.unwrap();
                        conn.write_all(&frame.bytes).await.unwrap();
                    }
                });
                anyhow::Ok(())
            });
        }
    });

    let event_rx = connect_to_upstream()?;

    let mut print_stats = crate::upstream::mk_stat_printer();

    info!("Starting runloop");
    for (frame, timestamp) in event_rx {
        print_stats(&frame, timestamp, 0);
        tx.send(frame)?;
    }

    Ok(())
}
