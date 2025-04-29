mod frame;
mod handshake;

pub use crate::frame::{Frame, NeedMoreBytes, OpCode};
use bytes::BytesMut;
use std::io::{BufReader, prelude::*};
use std::time::Duration;
use std::{fs::File, net::TcpStream, path::Path};
use thiserror::Error;
use url::Url;

pub fn read_websocket(
    path: &Path,
) -> Result<impl Iterator<Item = std::io::Result<Frame>>, ConnectionError> {
    Ok(read_frames(
        BufReader::new(File::open(path)?),
        BytesMut::with_capacity(8192),
        false,
    ))
}

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("Wrong code: expected 101, saw {0:?}")]
    WrongCode(Option<u16>),
    #[error("Missing header {0}")]
    MissingHeader(&'static str),
    #[error("Wrong value for header {header}: expected {expected}, saw {saw}")]
    WrongHeaderValue {
        header: &'static str,
        expected: &'static str,
        saw: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Httparse(#[from] httparse::Error),
    #[error(transparent)]
    InvalidDnsNameError(#[from] rustls::pki_types::InvalidDnsNameError),
    #[error(transparent)]
    Rustls(#[from] rustls::Error),
}

pub fn connect_websocket(
    url: &Url,
) -> Result<impl Iterator<Item = std::io::Result<Frame>> + Send + 'static, ConnectionError> {
    let host = url.host().unwrap().to_string();
    let port = url.port_or_known_default().unwrap();
    let mut conn = TcpStream::connect((host.as_str(), port))?;
    let tls = match url.scheme() {
        "ws" => false,
        "wss" => true,
        x => panic!("{x}: Unknown scheme"),
    };
    use crate::handshake::*;
    let mut buffer = BytesMut::with_capacity(8192);
    let conn: Box<dyn BufRead + Send> = if tls {
        let mut conn = tls_handshake(&host, conn)?;
        websocket_handshake_1(&mut conn, url)?;
        websocket_handshake_2(&mut conn, &mut buffer)?;
        Box::new(conn)
    } else {
        websocket_handshake_1(&mut conn, url)?;
        let mut conn = BufReader::new(conn);
        websocket_handshake_2(&mut conn, &mut buffer)?;
        Box::new(conn)
    };
    Ok(read_frames(conn, buffer, true))
}

fn read_frames(
    mut rdr: impl BufRead,
    mut buffer: BytesMut,
    stop_at_eof: bool,
) -> impl Iterator<Item = std::io::Result<Frame>> {
    std::iter::from_fn(move || read_frame(&mut rdr, &mut buffer, stop_at_eof).transpose())
        .take_while(|x| !x.as_ref().is_ok_and(|x| x.opcode() == OpCode::Close))
}

const MAX_FRAME_SIZE: usize = 64 << 20; // 64 MiB

fn read_frame(
    rdr: &mut impl BufRead,
    buffer: &mut BytesMut,
    stop_at_eof: bool,
) -> std::io::Result<Option<Frame>> {
    loop {
        match Frame::from_bytes(buffer) {
            Ok(x) => return Ok(Some(x)),
            Err(NeedMoreBytes(0)) => unreachable!(),
            Err(NeedMoreBytes(n)) if n > MAX_FRAME_SIZE => {
                return Err(std::io::Error::other(format!(
                    "Frame larger than max size: {n} bytes"
                )));
            }
            Err(NeedMoreBytes(_)) => {
                let m = fill_buffer(rdr, buffer)?;
                if m == 0 {
                    if stop_at_eof {
                        return Ok(None);
                    } else {
                        std::thread::sleep(Duration::from_millis(100))
                    }
                }
            }
        }
    }
}

fn fill_buffer(rdr: &mut impl BufRead, buffer: &mut BytesMut) -> std::io::Result<usize> {
    let slice = rdr.fill_buf()?;
    let m = slice.len();
    buffer.extend_from_slice(slice);
    rdr.consume(m);
    Ok(m)
}
