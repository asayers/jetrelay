use crate::BUFFER;
use crate::client::{Client, listen_for_clients};
use crate::upstream::{Timestamp, fake_iter};
use anyhow::{Context, Result};
use slab::Slab;
use std::io::{ErrorKind, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use tracing::*;
use wsclient::Frame;

pub fn run() -> Result<()> {
    let data = Arc::new(Mutex::new(Vec::<u8>::new()));

    // Bind the listener socket ASAP
    let var = "JETRELAY_PORT";
    let port: u16 = std::env::var(var).context(var)?.parse().context(var)?;
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr)?;
    info!(%listen_addr, "Bound socket");

    // Handle incoming client connections in a separate thread
    let (client_tx, client_rx) = mpsc::channel();
    let data_2 = data.clone();
    std::thread::Builder::new()
        .name("client_listener".to_owned())
        .spawn(move || {
            listen_for_clients(listener, client_tx, |mut conn| {
                let config = crate::handshake::perform_handshake(&mut conn)?;
                conn.set_nonblocking(true)?;
                let offset = config
                    .cursor
                    .and_then(crate::upstream::resolve_cursor)
                    .unwrap_or(data_2.lock().unwrap().len() as u64);
                debug!("Initial offset: {offset}");
                Ok(Client { conn, offset })
            })
        })?;

    let var = "UPSTREAM_URL";
    let ws_iter: Box<dyn Iterator<Item = Result<(Frame, Timestamp)>> + Send> =
        match std::env::var(var) {
            Ok(url) => {
                let url = url.parse().context(var)?;
                let frames = wsclient::connect_websocket(&url)?;
                Box::new(crate::upstream::jetstream_iter(frames))
            }
            Err(_) => Box::new(fake_iter()),
        };
    info!("Connected to upstream");

    let data_2 = data.clone();
    std::thread::Builder::new()
        .name("upstream_copier".to_owned())
        .spawn(move || {
            let mut print_stats = crate::upstream::mk_stat_printer();
            for x in ws_iter {
                let (frame, timestamp) = x?;
                let mut data = data_2.lock().unwrap();
                data.extend(&frame.bytes);
                let total = data.len() as u64;
                std::mem::drop(data);
                print_stats(&frame, timestamp, total);
            }
            anyhow::Ok(())
        })?;

    let mut clients = Slab::<Client>::default();

    info!("Starting runloop");
    loop {
        while let Ok(client) = client_rx.try_recv() {
            let client_id = clients.insert(client);
            debug!(client_id, "Client registered");
        }
        let data = data.lock().unwrap();
        let n_pages = (data.len() as u64) / BUFFER;
        clients.retain(|client_id, client| {
            let _g = info_span!("", client_id).entered();
            if client.offset / BUFFER < n_pages {
                let slice = &data[(client.offset as usize)..((n_pages * BUFFER) as usize)];
                let ret = client.conn.write(slice);
                match ret {
                    Ok(n) => client.offset += n as u64,
                    Err(e) => match e.kind() {
                        ErrorKind::WouldBlock => (), // Slow client
                        ErrorKind::BrokenPipe | ErrorKind::ConnectionReset => {
                            debug!("Socket closed by other side");
                            return false;
                        }
                        _ => panic!("{e:#}"),
                    },
                }
            }
            true
        });
        std::mem::drop(data);
        std::thread::sleep(Duration::from_millis(10))
    }
}
