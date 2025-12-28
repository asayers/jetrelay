use crate::BUFFER;
use crate::client::{Client, ClientAsync, listen_for_clients};
use crate::upstream::{Timestamp, fake_iter};
use anyhow::{Context, Result};
use io_uring::IoUring;
use io_uring::types::{Fd, Timespec};
use slab::Slab;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener};
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use tracing::*;
use wsclient::Frame;

pub fn run() -> Result<()> {
    // Set up the uring
    let mut uring: IoUring = IoUring::builder()
        .setup_single_issuer()
        .setup_defer_taskrun()
        .build(10240)
        .context("Build ring")?;
    info!("Set up the uring");

    let data = Arc::new(Mutex::new(Vec::<u8>::with_capacity(1024 * 1024 * 1024)));

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
                Ok(Client::new2(conn, config, &data_2).into())
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
                assert!(data.capacity() - data.len() > frame.bytes.len());
                data.extend(&frame.bytes);
                let total = data.len() as u64;
                std::mem::drop(data);
                print_stats(&frame, timestamp, total);
            }
            anyhow::Ok(())
        })?;

    let mut clients = Slab::<ClientAsync>::default();

    info!("Starting runloop");
    let mut last_print = Instant::now();
    let mut n_pushed = 0;
    let mut n_pulled = 0;
    loop {
        while let Ok(client) = client_rx.try_recv() {
            let client_id = clients.insert(client);
            debug!(client_id, "Client registered");
        }
        for cqe in uring.completion() {
            let client_id = cqe.user_data() as usize;
            n_pulled += 1;
            match cqe.result() {
                Ok(n) => {
                    let client = clients.get_mut(client_id).unwrap();
                    // if n != client.in_flight {
                    //     warn!(client.in_flight, n, "Huh?");
                    // }
                    client.in_flight = false;
                    client.inner.offset += n as u64;
                }
                Err(e) => match e.kind() {
                    ErrorKind::WouldBlock => {
                        // Slow client
                        let client = clients.get_mut(client_id).unwrap();
                        client.in_flight = false;
                    }
                    ErrorKind::BrokenPipe | ErrorKind::ConnectionReset => {
                        info!("Socket closed by other side");
                        clients.remove(client_id);
                    }
                    _ => panic!("{e:#}"),
                },
            }
        }

        let data = data.lock().unwrap();
        let n_pages = (data.len() as u64) / BUFFER;
        for (client_id, client) in &mut clients {
            let _g = info_span!("", client_id).entered();
            if client.in_flight {
                continue;
            }
            let mut sq = uring.submission();
            if sq.is_full() {
                break;
            }
            if client.inner.offset / BUFFER < n_pages {
                let slice = &data[(client.inner.offset as usize)..((n_pages * BUFFER) as usize)];
                let sqe = io_uring::opcode::Write::new(
                    Fd(client.inner.conn.as_raw_fd()),
                    slice.as_ptr(),
                    slice.len() as u32,
                )
                .rw_flags(rustix::io::ReadWriteFlags::NOWAIT.bits() as i32)
                .build()
                .user_data(client_id as u64);
                unsafe {
                    sq.push(&sqe).unwrap();
                }
                client.in_flight = true;
                n_pushed += 1;
            }
        }
        std::mem::drop(data);

        if last_print.elapsed() > Duration::from_secs(1) {
            {
                let sq = uring.submission();
                dbg!(
                    sq.taskrun(),
                    sq.len(),
                    sq.dropped(),
                    sq.cq_overflow(),
                    n_pushed,
                );
            }
            {
                let cq = uring.completion();
                dbg!(cq.overflow(), n_pulled);
            }
            println!("{} clients", clients.len());
            n_pushed = 0;
            n_pulled = 0;
            last_print = Instant::now();
        }

        const RUNLOOP_TIMEOUT: Timespec = Timespec::new().sec(1);
        let submit_args = io_uring::types::SubmitArgs::new().timespec(&RUNLOOP_TIMEOUT);
        match uring.submitter().submit_with_args(1, &submit_args) {
            Ok(n) => info!(n, "Completion"),
            Err(e) => match e.raw_os_error() {
                Some(62) => info!("Timeout"),
                _ => return Err(anyhow::anyhow!(e).context("submit")),
            },
        }
    }
}
