use crate::client::{Client, ClientWithPipe, listen_for_clients};
use crate::upstream::connect_to_upstream;
use anyhow::{Context, Result, ensure};
use io_uring::IoUring;
use io_uring::types::Timespec;
use rustix::fd::AsRawFd;
use rustix::fs::{MemfdFlags, memfd_create};
use slab::Slab;
use std::fs::File;
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, mpsc};
use tracing::*;

pub fn run() -> Result<()> {
    // Set up the uring
    let mut uring = IoUring::builder()
        .setup_single_issuer()
        .setup_defer_taskrun()
        .build(10240)
        .context("Build ring")?;
    uring.submitter().register_files_sparse(1)?;
    let uring_fd = uring.as_raw_fd();
    info!(fd = uring_fd, "Set up the uring");

    let mut file = create_file()?;
    uring
        .submitter()
        .register_files_update(0, &[file.as_raw_fd()])?;
    debug!("Registered file with the uring");
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
                Ok(Client::new(conn, config, &file_len_2).try_into()?)
            })
        })?;

    let mut clients = Slab::<ClientWithPipe>::default();

    let event_rx = connect_to_upstream()?;

    let file_len_2 = file_len.clone();
    std::thread::Builder::new()
        .name("event_writer".to_owned())
        .spawn(move || {
            let _g = info_span!("upstream copier thread").entered();
            info!("Copying data from upstream");
            let mut print_stats = crate::upstream::mk_stat_printer();
            for (frame, ts) in event_rx {
                file.write_all(&frame.bytes)?;
                let n = frame.bytes.len() as u64;
                trace!("Wrote {n} bytes");
                let offset = file_len_2.fetch_add(n, Ordering::Release);
                print_stats(&frame, ts, offset);
            }
            anyhow::Ok(())
        })?;

    let mut sqes = Vec::new();

    info!("Starting runloop");
    loop {
        while let Ok(client) = client_rx.try_recv() {
            let client_id = clients.insert(client);
            ensure!(client_id <= u32::MAX as usize);
            info!(client_id, "Client registered");
        }
        for cqe in uring.completion() {
            crate::io::handle_completion(&mut clients, cqe).context("handle_completion")?;
        }
        let file_len = file_len.load(Ordering::Acquire);
        if sqes.is_empty() {
            for (client_id, client) in &mut clients {
                let client_id = u32::try_from(client_id)?;
                crate::io::get_client_caught_up(&mut sqes, file_len, client_id, client)
                    .context("get_client_caught_up")?;
            }
        }
        {
            let mut sq = uring.submission();
            let limit = (sq.capacity() - sq.len()).min(sqes.len());
            unsafe { sq.push_multiple(&sqes[..limit]).context("push_multiple")? };
            for sqe in sqes.drain(..limit) {
                debug!("Submit {sqe:?}")
            }
        }
        trace!("(Waiting for completions...)");
        const RUNLOOP_TIMEOUT: Timespec = Timespec::new().sec(1);
        let submit_args = io_uring::types::SubmitArgs::new().timespec(&RUNLOOP_TIMEOUT);
        match uring.submitter().submit_with_args(1, &submit_args) {
            Ok(_) => (),
            Err(e) => match e.raw_os_error() {
                Some(62) => (), // Timeout
                _ => return Err(anyhow::anyhow!(e).context("submit")),
            },
        }
    }
}

pub fn create_file() -> Result<File> {
    let var = "RUNTIME_DIRECTORY";
    match std::env::var(var) {
        Ok(dir) => {
            let path = PathBuf::from(dir).join("jetrelay.dat");
            info!("Creating a file at {}", path.display());
            Ok(File::options()
                .read(true)
                .append(true)
                .create_new(true)
                .open(path)?)
        }
        Err(std::env::VarError::NotPresent) => Ok(memfd_create(
            "jetrelay.dat",
            MemfdFlags::CLOEXEC | MemfdFlags::NOEXEC_SEAL,
        )?
        .into()),
        Err(e) => Err(e).context(var),
    }
}
