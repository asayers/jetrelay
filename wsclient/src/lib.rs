mod frame;
mod handshake;

pub use crate::frame::{Frame, NeedMoreBytes, OpCode};
use anyhow::bail;
use bytes::BytesMut;
use std::io::{BufReader, prelude::*};
use std::time::Duration;
use std::{fs::File, net::TcpStream, path::Path};
use url::Url;

pub fn read_websocket(path: &Path) -> anyhow::Result<impl Iterator<Item = anyhow::Result<Frame>>> {
    Ok(read_frames(
        BufReader::new(File::open(path)?),
        BytesMut::with_capacity(8192),
        false,
    ))
}

pub fn connect_websocket(
    url: &Url,
) -> anyhow::Result<impl Iterator<Item = anyhow::Result<Frame>> + Send + 'static> {
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
) -> impl Iterator<Item = anyhow::Result<Frame>> {
    std::iter::from_fn(move || read_frame(&mut rdr, &mut buffer, stop_at_eof).transpose())
        .take_while(|x| !x.as_ref().is_ok_and(|x| x.opcode() == OpCode::Close))
}

fn read_frame(
    rdr: &mut impl BufRead,
    buffer: &mut BytesMut,
    stop_at_eof: bool,
) -> anyhow::Result<Option<Frame>> {
    let mut fill_buffer = |buffer: &mut BytesMut| {
        let slice = rdr.fill_buf()?;
        let m = slice.len();
        buffer.extend_from_slice(slice);
        rdr.consume(m);
        anyhow::Ok(m)
    };

    loop {
        match Frame::from_bytes(buffer) {
            Ok(x) => return Ok(Some(x)),
            Err(NeedMoreBytes(0)) => unreachable!(),
            Err(NeedMoreBytes(n)) if n > 64 << 20 => bail!("Over-large message"),
            Err(NeedMoreBytes(_)) => {
                let m = fill_buffer(buffer)?;
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
