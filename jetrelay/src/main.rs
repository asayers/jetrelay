mod handshake;
mod io;
mod upstream;

use anyhow::{Context, Result};
use rustix::fd::{AsRawFd, OwnedFd};
use rustix_uring::IoUring;
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::prelude::*;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use tracing::*;
use tracing_subscriber::{EnvFilter, prelude::*};

/// Respects the following env vars:
///
/// * JETRELAY_PORT (required)
/// * RUNTIME_DIRECTORY (required)
/// * RUST_LOG
fn main() -> Result<()> {
    log_init();

    // Set up the uring
    let mut uring = IoUring::new(1024)?;
    uring.submitter().register_files_sparse(1)?;
    let uring_fd = uring.as_raw_fd();
    info!(fd = uring_fd, "Set up the uring");

    let var = "RUNTIME_DIRECTORY";
    let dir: PathBuf = std::env::var(var).context(var)?.into();
    let file = create_file(&dir, &uring)?;
    let file_len = Arc::new(AtomicU64::new(0));

    // Bind the listener socket.  We do this ASAP, so clients can start
    // connecting immediately. It's fine for them to connect even before the
    // file exists.  Of course, they won't recieve any data until it _does_
    // exist.
    let var = "JETRELAY_PORT";
    let port: u16 = std::env::var(var).context(var)?.parse().context(var)?;
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr)?;
    info!(%listen_addr, "Bound socket");

    // Handle incoming client connections in a separate thread
    let (client_tx, client_rx) = std::sync::mpsc::channel();
    let file_len_2 = file_len.clone();
    std::thread::Builder::new()
        .name("client_listener".to_owned())
        .spawn(move || listen_for_clients(listener, client_tx, file_len_2))?;

    let mut clients = HashMap::<ClientId, Client>::default();
    let mut next_client_id = 0;

    let url = "wss://jetstream2.us-west.bsky.network/subscribe".parse()?;
    // let url = "ws://localhost:6008/subscribe".parse()?;
    let ws_iter = wsclient::connect_websocket(&url)?;
    info!("Connected to upstream");
    let file_len_2 = file_len.clone();
    std::thread::Builder::new()
        .name("upstream_copier".to_owned())
        .spawn(move || crate::upstream::copy_frames_to_file(file, file_len_2, ws_iter).unwrap())?;

    let mut sqes = VecDeque::new();

    info!("Starting runloop");
    loop {
        while let Ok(client) = client_rx.try_recv() {
            let client_id = next_client_id;
            next_client_id += 1;
            let _g = info_span!("", client_id).entered();
            clients.insert(client_id, client);
            info!("Client registered");
        }
        for cqe in uring.completion() {
            crate::io::handle_completion(&mut clients, cqe).context("handle_completion")?;
        }
        let file_len = file_len.load(Ordering::Acquire);
        for (client_id, client) in &mut clients {
            crate::io::get_client_caught_up(&mut sqes, file_len, *client_id, client)
                .context("get_client_caught_up")?;
        }
        sqes.push_back(crate::io::timeout());
        unsafe {
            uring.submit_all(sqes.drain(..)).context("submit_all")?;
        }
        trace!("(Waiting for completions...)");
        uring.submit_and_wait(1).context("submit_and_wait")?;
    }
}

fn listen_for_clients(listener: TcpListener, client_tx: Sender<Client>, file_len: Arc<AtomicU64>) {
    std::thread::scope(|scope| {
        let _g = info_span!("client listener thread").entered();
        info!(socket = ?listener, "Listening for client connections");
        for conn in listener.incoming() {
            std::thread::Builder::new()
                .name("client_handshake".to_owned())
                .spawn_scoped(scope, || {
                    let _g = debug_span!("handshake thread").entered();
                    match init_client(&client_tx, conn, &file_len) {
                        Ok(()) => (),
                        Err(e) => error!("{e}"),
                    }
                })
                .unwrap();
        }
        error!("Listening socket was closed!");
        std::process::exit(1);
    });
}

type ClientId = u32;

#[derive(Debug)]
struct Client {
    conn: TcpStream,
    offset: u64,
    bytes_in_pipe: u64,
    copy_in_flight: bool,
    send_in_flight: bool,
    pipe_rdr: OwnedFd,
    pipe_wtr: OwnedFd,
}

impl Client {
    fn new(mut conn: TcpStream, file_len: &AtomicU64) -> Result<Client> {
        let peer_addr = conn.peer_addr()?;
        let local_addr = conn.local_addr()?;
        info!(
            %peer_addr,
            %local_addr,
            "New client connected",
        );

        let config = crate::handshake::perform_handshake(&mut conn)?;
        info!(cursor = config.cursor.map(|x| x.0), "Handshake complete");

        let offset = config
            .cursor
            .and_then(crate::upstream::resolve_cursor)
            .unwrap_or(file_len.load(Ordering::Acquire));
        info!("Initial offset: {offset}");

        if !config.wanted_collections.is_empty()
            || !config.wanted_dids.is_empty()
            || config.max_message_size_bytes != usize::MAX
        {
            warn!("Support for filtering is not implemented");
        }
        if config.compress {
            warn!("Support for compression is not implemented");
        }
        if config.require_hello {
            warn!("Interactive mode is not implemented");
        }

        let (pipe_rdr, pipe_wtr) = rustix::pipe::pipe()?;
        Ok(Client {
            conn,
            offset,
            bytes_in_pipe: 0,
            copy_in_flight: false,
            send_in_flight: false,
            pipe_rdr,
            pipe_wtr,
        })
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        trace!("Sending close frame to client");
        let close_frame = [0x88, 0x02, 0x03, 0xE8];
        let _ = self.conn.write_all(&close_frame);
        let _ = self.conn.flush();
        let _ = self.conn.shutdown(std::net::Shutdown::Both);
    }
}

fn init_client(
    client_tx: &Sender<Client>,
    conn: std::io::Result<TcpStream>,
    file_len: &AtomicU64,
) -> Result<()> {
    let client = Client::new(conn?, file_len)?;
    client_tx.send(client)?;
    // We could wake up the io_uring here... but we don't bother
    Ok(())
}

/// Respect `RUST_LOG`, falling back to INFO-level
fn log_init() {
    let filter = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();
    let writer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    tracing_subscriber::registry()
        .with(filter)
        .with(writer)
        .init();
}

fn create_file(dir: &Path, uring: &IoUring) -> Result<File> {
    let path = dir.join("jetrelay.dat");
    info!("Creating a file at {}", path.display());
    let file = File::options()
        .read(true)
        .append(true)
        .create_new(true)
        .open(path)?;
    uring
        .submitter()
        .register_files_update(0, &[file.as_raw_fd()])?;
    debug!("Registered file with the uring");
    Ok(file)
}
