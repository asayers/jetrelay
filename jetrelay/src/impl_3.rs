use crate::BUFFER;
use crate::client::{Client, listen_for_clients};
use crate::impl_5::create_file;
use crate::upstream::{connect_to_upstream, copy_frames_to_file};
use anyhow::{Context, Result};
use rustix::fs::sendfile;
use slab::Slab;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use tracing::*;

pub fn run() -> Result<()> {
    let file = create_file()?;
    let file_len = Arc::new(AtomicU64::new(0));

    // Bind the listener socket ASAP
    let var = "JETRELAY_PORT";
    let port: u16 = std::env::var(var).context(var)?.parse().context(var)?;
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr)?;
    info!(%listen_addr, "Bound socket");

    // Handle incoming client connections in a separate thread
    let (client_tx, client_rx) = mpsc::channel();
    let file_len_2 = file_len.clone();
    std::thread::Builder::new()
        .name("client_listener".to_owned())
        .spawn(move || {
            listen_for_clients(listener, client_tx, |mut conn| {
                let config = wsserver::perform_handshake(&mut conn)?;
                conn.set_nonblocking(true)?;
                Ok(Client::new(conn, config, &file_len_2))
            })
        })?;

    let event_rx = connect_to_upstream()?;

    let file_len_2 = file_len.clone();
    let file_2 = file.try_clone()?;
    std::thread::Builder::new()
        .name("event_writer".to_owned())
        .spawn(move || copy_frames_to_file(file_2, file_len_2, event_rx))?;
    info!("Connected to upstream");

    let mut clients = Slab::<Client>::default();

    info!("Starting runloop");
    loop {
        while let Ok(client) = client_rx.try_recv() {
            let client_id = clients.insert(client);
            debug!(client_id, "Client registered");
        }
        let file_len = file_len.load(Ordering::Acquire);
        let n_pages = file_len / BUFFER;
        clients.retain(|_, client| {
            let sent_pages = client.offset / BUFFER;
            if sent_pages < n_pages {
                let count = ((n_pages - sent_pages) * BUFFER) as usize;
                let ret = sendfile(&client.conn, &file, Some(&mut client.offset), count);
                if let Err(e) = ret {
                    match e.kind() {
                        ErrorKind::WouldBlock => (), // Slow client
                        ErrorKind::BrokenPipe | ErrorKind::ConnectionReset => {
                            debug!("Socket closed by other side");
                            return false;
                        }
                        _ => panic!("{e:#}"),
                    }
                }
            }
            true
        });
        std::thread::sleep(Duration::from_millis(10))
    }
}
