use anyhow::{anyhow, ensure};
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
) -> anyhow::Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>> {
    let root_store = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let server = ServerName::try_from(hostname)?.to_owned();
    let session = ClientConnection::new(std::sync::Arc::new(config), server)?;
    let stream = StreamOwned::new(session, stream);
    Ok(stream)
}

pub fn websocket_handshake_1(mut conn: impl Write, url: &Url) -> anyhow::Result<()> {
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
pub fn websocket_handshake_2(mut rdr: impl BufRead, buffer: &mut BytesMut) -> anyhow::Result<()> {
    let mut fill_buffer = |buffer: &mut BytesMut| {
        let slice = rdr.fill_buf()?;
        let m = slice.len();
        buffer.extend_from_slice(slice);
        rdr.consume(m);
        anyhow::Ok(m)
    };

    loop {
        fill_buffer(buffer)?;
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

fn validate_response(response: Response) -> anyhow::Result<()> {
    ensure!(response.code == Some(101)); // Switching Protocols
    let header = |name: &str| -> anyhow::Result<&[u8]> {
        response
            .headers
            .iter()
            .find(|x| x.name.eq_ignore_ascii_case(name))
            .map(|x| x.value)
            .ok_or_else(|| anyhow!("Missing header: {name}"))
    };
    ensure!(
        header("connection")?.eq_ignore_ascii_case(b"upgrade"),
        "Wrong connection header"
    );
    ensure!(
        header("upgrade")?.eq_ignore_ascii_case(b"websocket"),
        "Wrong upgrade header"
    );
    Ok(())
}
