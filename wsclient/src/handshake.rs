use crate::{ConnectionError, fill_buffer};
use base64::prelude::*;
use bytes::{Buf, BytesMut};
use httparse::Response;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned, pki_types::ServerName};
use std::io::prelude::*;
use std::net::TcpStream;
use url::Url;

pub fn tls_handshake(
    hostname: &str,
    stream: TcpStream,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, ConnectionError> {
    let root_store = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let server = ServerName::try_from(hostname)?.to_owned();
    let session = ClientConnection::new(std::sync::Arc::new(config), server)?;
    let stream = StreamOwned::new(session, stream);
    Ok(stream)
}

pub fn websocket_handshake_1(mut conn: impl Write, url: &Url) -> Result<(), ConnectionError> {
    const KEY_BYTES: usize = 16;
    let key = BASE64_STANDARD.encode(rand::random::<[u8; KEY_BYTES]>());

    write!(conn, "GET {}", url.path())?;
    if let Some(q) = url.query() {
        write!(conn, "?{q}")?;
    }
    writeln!(conn, " HTTP/1.1\r")?;
    if let Some(host) = url.host() {
        writeln!(conn, "Host: {host}\r")?;
    }
    writeln!(conn, "Connection: Upgrade\r")?;
    writeln!(conn, "Upgrade: websocket\r")?;
    writeln!(conn, "Sec-WebSocket-Version: 13\r")?;
    writeln!(conn, "Sec-WebSocket-Key: {key}\r")?;
    writeln!(conn, "\r")?; // No body
    Ok(())
}

// TODO: timeout
pub fn websocket_handshake_2(
    mut rdr: impl BufRead,
    buffer: &mut BytesMut,
) -> Result<(), ConnectionError> {
    loop {
        fill_buffer(&mut rdr, buffer)?;
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut response = httparse::Response::new(&mut headers);
        let n = match response.parse(buffer)? {
            httparse::Status::Complete(n) => n,
            httparse::Status::Partial => continue,
        };
        validate_response(response)?;
        buffer.advance(n);
        return Ok(());
    }
}

fn validate_response(response: Response) -> Result<(), ConnectionError> {
    if response.code != Some(101 /* Switching Protocols */) {
        return Err(ConnectionError::WrongCode(response.code));
    }
    check_header(&response, "connection", "upgrade")?;
    check_header(&response, "upgrade", "websocket")?;
    Ok(())
}

fn check_header(
    response: &Response,
    header: &'static str,
    expected: &'static str,
) -> Result<(), ConnectionError> {
    let value = response
        .headers
        .iter()
        .find(|x| x.name.eq_ignore_ascii_case(header))
        .ok_or_else(|| ConnectionError::MissingHeader(header))?
        .value;
    if !value.eq_ignore_ascii_case(expected.as_bytes()) {
        return Err(ConnectionError::WrongHeaderValue {
            header,
            expected,
            saw: String::from_utf8_lossy(value).into_owned(),
        });
    }
    Ok(())
}
