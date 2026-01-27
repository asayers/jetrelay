use crate::BUFFER;
use crate::client::{Client, ClientWithPipe, listen_for_clients};
use crate::impl_5::create_file;
use crate::upstream::{connect_to_upstream, copy_frames_to_file};
use anyhow::{Context, Result};
use rustix::pipe::{SpliceFlags, splice};
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
                Ok(Client::new(conn, config, &file_len_2).try_into()?)
            })
        })?;

    let event_rx = connect_to_upstream()?;

    let file_len_2 = file_len.clone();
    let file_2 = file.try_clone()?;
    let t = std::thread::current();
    std::thread::Builder::new()
        .name("event_writer".to_owned())
        .spawn(move || copy_frames_to_file(file_2, file_len_2, event_rx, t))?;
    info!("Connected to upstream");

    let mut clients = Slab::<ClientWithPipe>::default();

    info!("Starting runloop");
    loop {
        while let Ok(client) = client_rx.try_recv() {
            let client_id = clients.insert(client);
            debug!(client_id, "Client registered");
        }
        let file_len = file_len.load(Ordering::Acquire);
        let n_pages = file_len / BUFFER;
        let mut anyone_did_something = false;
        clients.retain(|_, client| {
            loop {
                let sent_pages = client.inner.offset / BUFFER;
                let mut did_something = false;
                if sent_pages < n_pages {
                    let count = ((n_pages - sent_pages) * BUFFER) as usize;
                    let ret = splice(
                        &file,
                        Some(&mut client.inner.offset),
                        &client.pipe_wtr,
                        None,
                        count,
                        SpliceFlags::NONBLOCK,
                    );
                    did_something |= ret.is_ok();
                    anyone_did_something |= ret.is_ok();
                    match ret {
                        Ok(n) => client.bytes_in_pipe += n as u64,
                        Err(e) => match e.kind() {
                            ErrorKind::WouldBlock => (), // Pipe full
                            _ => panic!("{e:#}"),
                        },
                    }
                }
                if client.bytes_in_pipe > 0 {
                    let ret = splice(
                        &client.pipe_rdr,
                        None,
                        &client.inner.conn,
                        None,
                        client.bytes_in_pipe as usize,
                        SpliceFlags::NONBLOCK,
                    );
                    did_something |= ret.is_ok();
                    anyone_did_something |= ret.is_ok();
                    match ret {
                        Ok(n) => client.bytes_in_pipe -= n as u64,
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
                if !did_something {
                    break;
                }
            }
            true
        });
        if !anyone_did_something {
            std::thread::sleep(Duration::from_millis(10))
        }
    }
}
