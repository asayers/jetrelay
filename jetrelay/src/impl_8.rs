use crate::client::{Client, ClientAsync, listen_for_clients};
use crate::upstream::connect_to_upstream;
use crate::{BUFFER, MAX_SEND};
use anyhow::{Context, Result};
use io_uring::IoUring;
use io_uring::types::Fd;
use slab::Slab;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use tracing::*;

pub fn run() -> Result<()> {
    // Set up the uring
    let mut uring: IoUring = IoUring::builder()
        .setup_single_issuer()
        .setup_defer_taskrun()
        .build(10240)
        .context("Build ring")?;
    info!("Set up the uring");

    let data = Arc::new(Mutex::new(Vec::<u8>::with_capacity(1024 * 1024 * 1024)));
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
                Ok(Client::new(conn, config, &file_len_2).into())
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
                assert!(data.capacity() - data.len() > frame.bytes.len());
                data.extend(&frame.bytes);
                std::mem::drop(data);
                let total = file_len_2.fetch_add(frame.bytes.len() as u64, Ordering::Release);
                print_stats(&frame, timestamp, total);
            }
            anyhow::Ok(())
        })?;

    let mut clients = Slab::<ClientAsync>::default();

    info!("Starting runloop");
    let mut last_print = Instant::now();
    loop {
        while let Ok(client) = client_rx.try_recv() {
            let client_id = clients.insert(client);
            debug!(client_id, "Client registered");
        }

        let mut n_pushed = 0;
        let file_len = file_len.load(Ordering::Acquire);
        let n_pages = file_len / BUFFER;
        for (client_id, client) in &mut clients {
            let _g = info_span!("", client_id).entered();
            if client.in_flight {
                continue;
            }
            let mut sq = uring.submission();
            if sq.is_full() {
                break;
            }
            let last_page = client.inner.offset / BUFFER;
            if last_page < n_pages {
                let data = data.lock().unwrap();
                let slice = &data[(client.inner.offset as usize)
                    ..(((last_page + MAX_SEND).min(n_pages) * BUFFER) as usize)];
                let sqe = io_uring::opcode::SendZc::new(
                    Fd(client.inner.conn.as_raw_fd()),
                    slice.as_ptr(),
                    slice.len() as u32,
                )
                // .flags(rustix::net::SendFlags::DONTWAIT.bits() as i32)
                .build()
                .user_data(client_id as u64);
                std::mem::drop(data);
                // let sqe = io_uring::opcode::Write::new(
                //     Fd(client.inner.conn.as_raw_fd()),
                //     slice.as_ptr(),
                //     slice.len() as u32,
                // )
                // .rw_flags(rustix::io::ReadWriteFlags::NOWAIT.bits() as i32)
                // .build()
                // .user_data(client_id as u64);
                unsafe {
                    sq.push(&sqe).unwrap();
                }
                client.in_flight = true;
                n_pushed += 1;
            }
        }

        // const RUNLOOP_TIMEOUT: Timespec = Timespec::new().sec(1);
        // let submit_args = io_uring::types::SubmitArgs::new().timespec(&RUNLOOP_TIMEOUT);
        // match uring.submitter().submit_with_args(1, &submit_args) {
        //     Ok(n) => info!(n, "Completion"),
        //     Err(e) => match e.raw_os_error() {
        //         Some(62) => info!("Timeout"),
        //         _ => return Err(anyhow::anyhow!(e).context("submit")),
        //     },
        // }
        // info!("submitting...");
        uring.submitter().submit().context("submit")?;
        // info!("submitted");

        let mut n_pulled = 0;
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
        if n_pushed != n_pulled {
            warn!(n_pushed, n_pulled);
        }

        if last_print.elapsed() > Duration::from_secs(1) {
            {
                let sq = uring.submission();
                dbg!(sq.taskrun(), sq.len(), sq.dropped(), sq.cq_overflow(),);
            }
            {
                let cq = uring.completion();
                dbg!(cq.overflow());
            }
            println!("{} clients", clients.len());
            last_print = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(10))
    }
}
