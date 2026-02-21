use anyhow::{Context, Result, bail, ensure};
use io_uring::squeue::Flags;
use io_uring::types::Timespec;
use io_uring::{IoUring, opcode};
use io_uring::{cqueue, squeue};
use rustix::fd::AsRawFd;
use slab::Slab;
use std::net::{SocketAddr, TcpListener};
use std::os::fd::IntoRawFd;
use std::sync::{LazyLock, Mutex};
use tracing::*;
use tracing_subscriber::{EnvFilter, prelude::*};
use wsclient::OpCode;
// use wsserver::ClientConfig;

static DATA: LazyLock<Mutex<Vec<u8>>> = LazyLock::new(|| Mutex::new(Vec::with_capacity(MAX_DATA)));
const MAX_DATA: usize = 1024 * 1024 * 1024;

const MAX_FILES: u32 = 100_000;
const LISTENER: io_uring::types::Fixed = io_uring::types::Fixed(MAX_FILES - 1);

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
    // uring.submitter().register_buffers_sparse(1)?;
    // unsafe {
    //     let data = DATA.lock().unwrap();
    //     let iovec = IoSlice::new(&data);
    //     uring
    //         .submitter()
    //         .register_buffers(&[std::mem::transmute(iovec)])?;
    // }
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
    uring
        .submitter()
        .register_files_update(LISTENER.0, &[listener.into_raw_fd()])
        .context("Registering listener")?;

    unsafe {
        let op = io_uring::opcode::AcceptMulti::new(LISTENER)
            .allocate_file_index(true)
            .build()
            .user_data(mk_user_data(CODE_ACCEPT, 0));
        uring.submission().push(&op)?;
    }

    let mut clients = Slab::<Client>::default();

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
                // trace!("Extended DATA to {} B", to);
            }
            anyhow::Ok(())
        })?;

    let mut sqes = Vec::new();

    info!("Starting runloop");
    loop {
        let mut n_completed = 0;
        for cqe in uring.completion() {
            handle_completion(&mut clients, cqe, &mut sqes).context("handle_completion")?;
            n_completed += 1;
        }
        let data_len = DATA.lock().unwrap().len();
        let n_pages = (data_len / 4096) as u32;
        if sqes.is_empty() {
            for (client_id, client) in &mut clients {
                let client_id = u32::try_from(client_id)?;
                get_client_caught_up(&mut sqes, n_pages, client_id, client)
                    .context("get_client_caught_up")?;
            }
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
    conn: io_uring::types::Fixed,
    offset: u64,
    in_flight: bool,
    buffer: Option<(Box<[u8]>, usize)>,
}

impl Client {
    fn mk_read(&mut self) -> squeue::Entry {
        let buf = self.buffer.as_mut().unwrap();
        opcode::Read::new(
            self.conn,
            buf.0[buf.1..].as_mut_ptr(),
            (buf.0.len() - buf.1) as u32,
        )
        .build()
    }
}

// impl Drop for Client {
//     fn drop(&mut self) {
//         trace!("Sending close frame to client");
//         let close_frame = [0x88, 0x02, 0x03, 0xE8];
//         let _ = self.conn.write_all(&close_frame);
//         let _ = self.conn.flush();
//         let _ = self.conn.shutdown(std::net::Shutdown::Both);
//     }
// }

fn get_client_caught_up(
    sqes: &mut Vec<squeue::Entry>,
    n_pages: u32,
    client_id: ClientId,
    client: &mut Client,
) -> Result<()> {
    let last_page = (client.offset / 4096) as u32;
    if !client.in_flight && last_page < n_pages && client.buffer.is_none() {
        let new_pages = n_pages - last_page;
        let n = new_pages * 4096;
        let from = client.offset as usize;
        let to = from + n as usize;
        let slice = &DATA.lock().unwrap()[client.offset as usize..];
        // let op = opcode::Send::new(client.conn, slice.as_ptr(), slice.len() as u32)
        let op = opcode::SendZc::new(client.conn, slice.as_ptr(), slice.len() as u32)
            .build()
            .user_data(mk_user_data(CODE_SEND_DATA, client_id));
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

fn kill_client(sqes: &mut Vec<squeue::Entry>, clients: &Slab<Client>, client_id: u32) {
    static FOO: [i32; 1] = [-1];
    static CLOSE_FRAME: [u8; 4] = [0x88, 0x02, 0x03, 0xE8];
    let client = &clients[client_id as usize];
    sqes.extend([
        opcode::Write::new(client.conn, CLOSE_FRAME.as_ptr(), CLOSE_FRAME.len() as u32)
            .build()
            .flags(Flags::IO_HARDLINK)
            .user_data(client_id as u64),
        opcode::Close::new(client.conn)
            .build()
            .flags(Flags::IO_HARDLINK)
            .user_data(client_id as u64),
        opcode::FilesUpdate::new(FOO.as_ptr(), 1)
            .offset(client.conn.0 as i32)
            .build()
            .user_data(mk_user_data(CODE_REMOVE_FD, client_id)),
    ])
}

const CODE_ACCEPT: u32 = 1;
const CODE_READ_HDRS: u32 = 2;
const CODE_WRITE_HDRS: u32 = 3;
const CODE_REMOVE_FD: u32 = 4;
const CODE_SEND_DATA: u32 = 5;

fn mk_user_data(code: u32, client_id: u32) -> u64 {
    (code as u64) << 32 | client_id as u64
}

fn handle_completion(
    clients: &mut Slab<Client>,
    cqe: cqueue::Entry,
    sqes: &mut Vec<squeue::Entry>,
) -> Result<()> {
    let code = (cqe.user_data() >> 32) as u32;
    let client_id = cqe.user_data() as u32;
    match code {
        // 1 => {
        //     assert!(cqueue::more(cqueue::Entry::flags(&cqe)));
        //     let mut conn = unsafe { TcpStream::from_raw_fd(cqe.result()? as i32) };
        //     let peer_addr = conn.peer_addr()?;
        //     let local_addr = conn.local_addr()?;
        //     debug!(
        //         %peer_addr,
        //         %local_addr,
        //         "New client connected",
        //     );
        //     let config = wsserver::perform_handshake(&mut conn)?;
        //     let client = Client::new(conn, config).try_into()?;
        //     let client_id = clients.insert(client);
        //     ensure!(client_id <= u32::MAX as usize);
        //     info!(client_id, "Client registered");
        // }
        CODE_ACCEPT => {
            if !cqueue::more(cqueue::Entry::flags(&cqe)) {
                warn!("Re-arming accept");
                sqes.push(
                    io_uring::opcode::AcceptMulti::new(LISTENER)
                        .allocate_file_index(true)
                        .build()
                        .user_data(mk_user_data(CODE_ACCEPT, 0)),
                );
            }
            let conn = io_uring::types::Fixed(cqe.result()?);
            // let peer_addr = conn.peer_addr()?;
            // let local_addr = conn.local_addr()?;
            debug!(
                //     %peer_addr,
                //     %local_addr,
                "New client connected",
            );
            let client_id = clients.insert(Client {
                conn,
                offset: 0,
                in_flight: false,
                buffer: Some((Box::new([0; 4096]), 0)),
            });
            ensure!(client_id <= u32::MAX as usize);
            debug!(client_id, conn = conn.0, "Client registered");
            let client = &mut clients[client_id];
            sqes.push(
                client
                    .mk_read()
                    .user_data(mk_user_data(CODE_READ_HDRS, client_id as u32)),
            );
        }
        CODE_READ_HDRS => {
            let client = &mut clients[client_id as usize];
            let n = cqe.result()?;
            let buf = client.buffer.as_mut().unwrap();
            buf.1 += n as usize;
            match wsserver::parse_headers(&buf.0[..buf.1]) {
                Ok(Some((resp, config))) => {
                    client.offset = match config.cursor {
                        Some(_) => todo!(),
                        None => DATA.lock().unwrap().len() as u64,
                    };
                    debug!(client_id, "Initial offset: {}", client.offset);
                    let resp: Box<[u8]> = resp.into_bytes().into();
                    sqes.push(
                        opcode::Write::new(client.conn, resp.as_ptr(), resp.len() as u32)
                            .build()
                            .user_data(mk_user_data(CODE_WRITE_HDRS, client_id)),
                    );
                    client.buffer = Some((resp, 0));
                }
                Ok(None) => {
                    sqes.push(
                        client
                            .mk_read()
                            .user_data(mk_user_data(CODE_READ_HDRS, client_id)),
                    );
                }
                Err(_) => {
                    kill_client(sqes, clients, client_id);
                    // let mut client = clients.remove(client_id as usize);
                    // let conn = &mut client.conn;
                    // writeln!(conn, "HTTP/1.1 500 {e:#}\r")?;
                    // writeln!(conn, "\r")?;
                    // conn.flush()?;
                    // conn.shutdown(std::net::Shutdown::Both)?;
                }
            }
        }
        3 => {
            let client = &mut clients[client_id as usize];
            match cqe.result() {
                Ok(n) => {
                    debug!(
                        client_id,
                        "Send {n} bytes of headers.  Client is now ready to recieve data"
                    );
                    // info!("Client connected");
                    client.buffer = None;
                }
                Err(e) => {
                    error!(client_id, "{e:#}");
                    kill_client(sqes, clients, client_id);
                }
            }
        }
        4 => {
            clients.remove(client_id as usize);
            info!(client_id, "Unregistered client");
        }
        5 => {
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
                    kill_client(sqes, clients, client_id);
                }
            }
        }
        0 => match cqe.result() {
            Ok(n) => info!(client_id, "Some kind of I/O completed: {n}"),
            Err(e) => warn!(client_id, "Some kind of I/O completed: {e:#}"),
        },
        _ => panic!("{}", code),
    }
    Ok(())
}
