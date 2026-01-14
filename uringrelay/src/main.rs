use anyhow::{Context, Result, bail, ensure};
use io_uring::types::Timespec;
use io_uring::{IoUring, opcode};
use io_uring::{cqueue, squeue};
use rustix::fd::AsRawFd;
use slab::Slab;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::fd::{FromRawFd, IntoRawFd};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::*;
use tracing_subscriber::{EnvFilter, prelude::*};
use wsclient::OpCode;
use wsserver::ClientConfig;

static DATA_LEN: AtomicU64 = AtomicU64::new(0);
static DATA: Mutex<[u8; MAX_DATA]> = Mutex::new([0; MAX_DATA]);
const MAX_DATA: usize = 1024 * 1024 * 1024;

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
        .setup_defer_taskrun()
        .build(1024)
        .context("Build ring")?;
    uring.submitter().register_files_sparse(1)?;
    let uring_fd = uring.as_raw_fd();
    info!(fd = uring_fd, "Set up the uring");

    // unsafe {
    //     let mut data = DATA.lock().unwrap();
    //     uring
    //         .submitter()
    //         .register_buffers(&[libc::iovec {
    //             iov_base: data.as_mut_ptr() as _,
    //             iov_len: MAX_DATA,
    //         }])
    //         .context("Register buf")?
    // };
    // debug!("Registered buffer with the uring");

    // Bind the listener socket ASAP
    let var = "JETRELAY_PORT";
    let port: u16 = std::env::var(var).context(var)?.parse().context(var)?;
    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), port);
    let listener = TcpListener::bind(listen_addr)?;
    info!(%listen_addr, "Bound socket");

    unsafe {
        let fd = listener.into_raw_fd();
        let op = io_uring::opcode::AcceptMulti::new(io_uring::types::Fd(fd))
            .build()
            .user_data(u64::MAX);
        uring.submission().push(&op)?;
    }

    let mut clients = Slab::<Client>::default();

    let var = "UPSTREAM_URL";
    let url = std::env::var(var).context(var)?.parse().context(var)?;
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
                let from = DATA_LEN.load(Ordering::Acquire) as usize;
                let to = from + frame.bytes.len() as usize;
                assert!(to < MAX_DATA);
                data[from..to].copy_from_slice(&frame.bytes);
                DATA_LEN.store(to as u64, Ordering::Release);
                trace!("Wrote DATA[{from}..{to}]");
            }
            anyhow::Ok(())
        })?;

    let mut sqes = Vec::new();

    info!("Starting runloop");
    let data_ptr = DATA.lock().unwrap().as_ptr();
    loop {
        for cqe in uring.completion() {
            handle_completion(&mut clients, cqe).context("handle_completion")?;
        }
        let data_len = DATA_LEN.load(Ordering::Acquire);
        let n_pages = (data_len / 4096) as u32;
        trace!("Data len: {data_len} B = {n_pages} pages");
        if sqes.is_empty() {
            for (client_id, client) in &mut clients {
                let client_id = u32::try_from(client_id)?;
                get_client_caught_up(&mut sqes, data_ptr, n_pages, client_id, client)
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

type ClientId = u32;

#[derive(Debug)]
struct Client {
    conn: TcpStream,
    offset: u64,
    in_flight: bool,
}

impl Client {
    fn new(conn: TcpStream, config: ClientConfig) -> Client {
        let offset = match config.cursor {
            Some(_) => todo!(),
            None => DATA_LEN.load(Ordering::Acquire),
        };
        debug!("Initial offset: {offset}");
        Client {
            conn,
            offset,
            in_flight: false,
        }
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

fn get_client_caught_up(
    sqes: &mut Vec<squeue::Entry>,
    data_ptr: *const u8,
    n_pages: u32,
    client_id: ClientId,
    client: &mut Client,
) -> Result<()> {
    let _g = debug_span!("", client_id).entered();
    let last_page = (client.offset / 4096) as u32;
    if !client.in_flight && last_page < n_pages {
        let new_pages = n_pages - last_page;
        let op = opcode::SendZc::new(
            io_uring::types::Fd(client.conn.as_raw_fd()),
            unsafe { data_ptr.add(client.offset as usize) },
            new_pages * 4096,
        )
        // .buf_index(Some(0))
        .build()
        .user_data(client_id as u64);
        sqes.push(op);
        client.in_flight = true;
    }
    Ok(())
}

fn handle_completion(clients: &mut Slab<Client>, cqe: cqueue::Entry) -> Result<()> {
    if cqe.user_data() == u64::MAX {
        assert!(cqueue::more(cqueue::Entry::flags(&cqe)));
        let mut conn = unsafe { TcpStream::from_raw_fd(cqe.result()? as i32) };
        let peer_addr = conn.peer_addr()?;
        let local_addr = conn.local_addr()?;
        debug!(
            %peer_addr,
            %local_addr,
            "New client connected",
        );
        let config = wsserver::perform_handshake(&mut conn)?;
        let client = Client::new(conn, config).try_into()?;
        let client_id = clients.insert(client);
        ensure!(client_id <= u32::MAX as usize);
        info!(client_id, "Client registered");
    } else {
        let client_id = cqe.user_data() as u32;
        let result = cqe.result();
        debug!(client_id, "I/O completed with {result:?}");
        match result {
            _ if cqueue::notif(cqe.flags()) => {
                assert_eq!(result?, 0);
                info!("This CQE is a notification")
            }
            Ok(n) => {
                let client = &mut clients[client_id as usize];
                client.offset += n as u64;
                client.in_flight = false;
            }
            Err(e) => error!("{e:#}"),
        }
    }
    Ok(())
}
