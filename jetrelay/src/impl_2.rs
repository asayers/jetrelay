use crate::BUFFER;
use crate::client::{Client, listen_for_clients};
use crate::upstream::connect_to_upstream;
use anyhow::{Context, Result};
use slab::Slab;
use std::io::{ErrorKind, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;
use tracing::*;

pub fn run() -> Result<()> {
    let data = Arc::new(Mutex::new(Vec::<u8>::new()));
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
                let config = crate::handshake::perform_handshake(&mut conn)?;
                conn.set_nonblocking(true)?;
                Ok(Client::new(conn, config, &file_len_2))
            })
        })?;

    let event_rx = connect_to_upstream()?;

    let data_2 = data.clone();
    let file_len_2 = file_len.clone();
    std::thread::Builder::new()
        .name("event_writer".to_owned())
        .spawn(move || {
            let mut print_stats = crate::upstream::mk_stat_printer();
            for (frame, timestamp) in event_rx {
                let mut data = data_2.lock().unwrap();
                data.extend(&frame.bytes);
                std::mem::drop(data);
                let total = file_len_2.fetch_add(frame.bytes.len() as u64, Ordering::Release);
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
        let file_len = file_len.load(Ordering::Acquire);
        let n_pages = file_len / BUFFER;
        let mut n_writes = 0;
        let mut bytes_written = 0;
        clients.retain(|client_id, client| {
            let _g = info_span!("", client_id).entered();
            if client.offset / BUFFER < n_pages {
                let data = data.lock().unwrap();
                let last_page = client.offset / BUFFER;
                const MAX_SEND: u64 = 64; // pages
                let slice = &data[(client.offset as usize)
                    ..(((last_page + MAX_SEND).min(n_pages) * BUFFER) as usize)];
                // let slice = &data[(client.offset as usize)..((n_pages * BUFFER) as usize)];
                let ret = client.conn.write(slice);
                n_writes += 1;
                match ret {
                    Ok(n) => {
                        client.offset += n as u64;
                        bytes_written += n;
                    }
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
        // dbg!(n_writes, bytes_written / 1024 / 1024);
        std::thread::sleep(Duration::from_millis(10))
    }
}
