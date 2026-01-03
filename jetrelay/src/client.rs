use crate::handshake::ClientConfig;
use anyhow::Result;
use rustix::pipe::PipeFlags;
use std::io::{PipeReader, PipeWriter, prelude::*};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use tracing::*;

pub fn listen_for_clients<C: Send + Sync + 'static>(
    listener: TcpListener,
    client_tx: Sender<C>,
    mk_client: impl Fn(TcpStream) -> Result<C> + Send + Sync,
) {
    std::thread::scope(|scope| {
        let _g = info_span!("client listener thread").entered();
        info!(socket = ?listener, "Listening for client connections");
        for conn in listener.incoming() {
            std::thread::Builder::new()
                .name("client_handshake".to_owned())
                .spawn_scoped(scope, || {
                    let _g = debug_span!("handshake thread").entered();
                    match init_client(&client_tx, conn, &mk_client) {
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

fn init_client<C: Send + Sync + 'static>(
    client_tx: &Sender<C>,
    conn: std::io::Result<TcpStream>,
    mk_client: impl Fn(TcpStream) -> Result<C>,
) -> Result<()> {
    let conn = conn?;
    let peer_addr = conn.peer_addr()?;
    let local_addr = conn.local_addr()?;
    debug!(
        %peer_addr,
        %local_addr,
        "New client connected",
    );

    let client = mk_client(conn)?;
    client_tx.send(client)?;
    // We could wake up the io_uring here... but we don't bother
    Ok(())
}

pub type ClientId = u32;

#[derive(Debug)]
pub struct Client {
    pub conn: TcpStream,
    pub offset: u64,
}

impl Client {
    pub fn new(conn: TcpStream, config: ClientConfig, file_len: &AtomicU64) -> Client {
        let offset = config
            .cursor
            .and_then(crate::upstream::resolve_cursor)
            .unwrap_or_else(|| file_len.load(Ordering::Acquire));
        debug!("Initial offset: {offset}");
        Client { conn, offset }
    }

    pub fn new2(conn: TcpStream, config: ClientConfig, file: &Mutex<Vec<u8>>) -> Client {
        let offset = config
            .cursor
            .and_then(crate::upstream::resolve_cursor)
            .unwrap_or_else(|| file.lock().unwrap().len() as u64);
        debug!("Initial offset: {offset}");
        Client { conn, offset }
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

#[derive(Debug)]
pub struct ClientAsync {
    pub inner: Client,
    pub in_flight: bool,
}

impl From<Client> for ClientAsync {
    fn from(inner: Client) -> Self {
        ClientAsync {
            inner,
            in_flight: false,
        }
    }
}

#[derive(Debug)]
pub struct ClientWithPipe {
    pub inner: Client,
    pub bytes_in_pipe: u64,
    pub copy_in_flight: bool,
    pub send_in_flight: bool,
    pub pipe_rdr: PipeReader,
    pub pipe_wtr: PipeWriter,
}

impl TryFrom<Client> for ClientWithPipe {
    type Error = std::io::Error;
    fn try_from(inner: Client) -> std::io::Result<Self> {
        // let (pipe_rdr, pipe_wtr) = std::io::pipe()?;
        let (pipe_rdr, pipe_wtr) = rustix::pipe::pipe_with(PipeFlags::NONBLOCK)?;
        Ok(ClientWithPipe {
            inner,
            bytes_in_pipe: 0,
            copy_in_flight: false,
            send_in_flight: false,
            pipe_rdr: pipe_rdr.into(),
            pipe_wtr: pipe_wtr.into(),
        })
    }
}
