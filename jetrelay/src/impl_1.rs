use crate::client::listen_for_clients;
use crate::upstream::connect_to_upstream;
use anyhow::{Context, Result};
use slab::Slab;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;
use tracing::*;
use wsclient::Frame;

pub fn run() -> Result<()> {
    // Bind the listener socket ASAP
    let var = "JETRELAY_PORT";
    let port: u16 = std::env::var(var).context(var)?.parse().context(var)?;
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr)?;
    info!(%listen_addr, "Bound socket");

    // Handle incoming client connections in a separate thread
    let (client_tx, client_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("client_listener".to_owned())
        .spawn(move || {
            listen_for_clients(listener, client_tx, |mut conn| {
                let _config = crate::handshake::perform_handshake(&mut conn)?;
                let (tx, rx) = mpsc::channel::<Frame>();
                std::thread::spawn(move || {
                    for frame in rx {
                        conn.write_all(&frame.bytes).unwrap();
                    }
                });
                Ok(tx)
            })
        })?;

    let event_rx = connect_to_upstream()?;

    let mut print_stats = crate::upstream::mk_stat_printer();
    let mut clients = Slab::<mpsc::Sender<Frame>>::default();

    info!("Starting runloop");
    for (frame, timestamp) in event_rx {
        print_stats(&frame, timestamp, 0);
        clients.retain(|_, client| client.send(frame.clone()).is_ok());
        while let Ok(client) = client_rx.try_recv() {
            let client_id = clients.insert(client);
            debug!(client_id, "Client registered");
        }
    }

    Ok(())
}
