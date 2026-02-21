use anyhow::{Context, Result, bail, ensure};
use io_uring::types::Timespec;
use io_uring::{IoUring, opcode};
use io_uring::{cqueue, squeue};
use rustix::fd::AsRawFd;
use slab::Slab;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{LazyLock, Mutex};
use tracing::*;
use tracing_subscriber::{EnvFilter, prelude::*};
use wsclient::OpCode;

static DATA: LazyLock<Mutex<Vec<u8>>> = LazyLock::new(|| Mutex::new(Vec::with_capacity(MAX_DATA)));
const MAX_DATA: usize = 1024 * 1024 * 1024;

const MAX_FILES: u32 = 100_000;

fn main() -> Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();
    let writer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    tracing_subscriber::registry()
        .with(filter)
        .with(writer)
        .init();

    // Set up the uring
    let mut uring = IoUring::builder()
        .setup_single_issuer()
        .setup_coop_taskrun()
        .setup_defer_taskrun()
        .build(8192)
        .context("Build ring")?;
    uring.submitter().register_files_sparse(MAX_FILES)?;
    info!(fd = uring.as_raw_fd(), "Set up the uring");

    // Bind the listener socket ASAP
    let var = "JETRELAY_PORT";
    let port: u16 = match std::env::var(var) {
        Ok(x) => x.parse()?,
        Err(_) => 7375,
    };
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr)?;
    info!(%listen_addr, "Bound socket");

    let var = "UPSTREAM_URL";
    let url = match std::env::var(var) {
        Ok(x) => x.parse()?,
        Err(_) => "ws://localhost:7376/subscribe".parse()?,
    };
    let frames = wsclient::connect_websocket(&url)?;
    info!("Connected to upstream");

    std::thread::Builder::new()
        .name("event_writer".to_owned())
        .spawn(move || {
            let _g = info_span!("upstream copier thread").entered();
            info!("Copying data from upstream");
            for frame in frames {
                let frame = frame.context("I/O error while reading from websocket")?;
                ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
                ensure!(frame.mask().is_none(), "Frame is masked");
                match frame.opcode() {
                    OpCode::Text => (),       // expected
                    OpCode::Ping => continue, // Ignore
                    OpCode::Close => bail!("Upstream is shutting us down :-("),
                    OpCode::Binary => bail!("Binary frame: {frame:?}"),
                    x => bail!("Unexpected opcode: {x:?}"),
                }
                let mut data = DATA.lock().unwrap();
                let to = data.len() + frame.bytes.len() as usize;
                assert!(to < MAX_DATA);
                data.extend_from_slice(&frame.bytes);
            }
            anyhow::Ok(())
        })?;

    let (new_clients_tx, new_clients_rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || -> anyhow::Result<()> {
        loop {
            let (mut conn, addr) = listener.accept()?;
            debug!(%addr, "New client connected");
            let _config = wsserver::perform_handshake(&mut conn)?;
            let client = Client {
                conn,
                offset: DATA.lock().unwrap().len() as u64,
                in_flight: false,
            };
            new_clients_tx.send(client)?;
        }
    });

    let mut sqes = Vec::new();
    let mut clients = Slab::<Client>::default();

    info!("Starting runloop");
    loop {
        let mut n_completed = 0;
        for cqe in uring.completion() {
            handle_completion(cqe, &mut clients).context("handle_completion")?;
            n_completed += 1;
        }
        for client in new_clients_rx.try_iter() {
            let fd = client.conn.as_raw_fd();
            let client_id = clients.insert(client);
            ensure!(client_id < MAX_FILES as usize);
            debug!(client_id, "Client registered");
            uring
                .submitter()
                .register_files_update(client_id as u32, &[fd])?;
        }
        let data_len = DATA.lock().unwrap().len();
        let n_pages = (data_len / 4096) as u32;
        for (client_id, client) in &mut clients {
            let client_id = u32::try_from(client_id)?;
            get_client_caught_up(&mut sqes, n_pages, client_id, client)
                .context("get_client_caught_up")?;
        }
        let n_submitted = {
            let mut sq = uring.submission();
            let limit = (sq.capacity() - sq.len()).min(sqes.len());
            unsafe { sq.push_multiple(&sqes[..limit]).context("push_multiple")? };
            for sqe in sqes.drain(..limit) {
                debug!("Submit {sqe:?}")
            }
            limit
        };
        trace!("(Waiting for completions...)");
        const RUNLOOP_TIMEOUT: Timespec = Timespec::new().sec(1);
        let submit_args = io_uring::types::SubmitArgs::new().timespec(&RUNLOOP_TIMEOUT);
        match uring.submitter().submit_with_args(1, &submit_args) {
            Ok(_) => (),
            Err(e) => match e.raw_os_error() {
                Some(62) => info!(
                    "Timeout.  n_submitted={n_submitted}, n_completed={n_completed}. Data len: {:.2} MiB.  Clients: {}",
                    data_len as f64 / 1024. / 1024.,
                    clients.len(),
                ),
                _ => return Err(anyhow::anyhow!(e).context("submit")),
            },
        }
    }
}

type ClientId = u32;

#[derive(Debug)]
struct Client {
    conn: TcpStream,
    offset: u64,
    in_flight: bool,
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

fn get_client_caught_up(
    sqes: &mut Vec<squeue::Entry>,
    n_pages: u32,
    client_id: ClientId,
    client: &mut Client,
) -> Result<()> {
    let last_page = (client.offset / 4096) as u32;
    if !client.in_flight && last_page < n_pages {
        let new_pages = n_pages - last_page;
        let n = new_pages * 4096;
        let from = client.offset as usize;
        let to = from + n as usize;
        let slice = &DATA.lock().unwrap()[client.offset as usize..to];
        let op = opcode::SendZc::new(
            io_uring::types::Fixed(client_id),
            slice.as_ptr(),
            slice.len() as u32,
        )
        .build()
        .user_data(client_id as u64);
        sqes.push(op);
        client.in_flight = true;
        debug!(
            client_id,
            offset = client.offset,
            "Submitted write of DATA[{from}..{to}] ({n} bytes)"
        );
    }
    Ok(())
}

fn handle_completion(cqe: cqueue::Entry, clients: &mut Slab<Client>) -> Result<()> {
    let client_id = cqe.user_data() as u32;
    let result = cqe.result();
    match result {
        _ if cqueue::notif(cqe.flags()) => {
            assert_eq!(result?, 0);
            debug!(client_id, "Notification");
        }
        Ok(n) => {
            let client = &mut clients[client_id as usize];
            client.offset += n as u64;
            client.in_flight = false;
            debug!(client_id, offset = client.offset, "Sent {n} bytes");
        }
        Err(e) => {
            error!(client_id, "Send: {e:#}");
            clients.remove(client_id as usize);
        }
    }
    Ok(())
}
