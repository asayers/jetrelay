use crate::upstream::Timestamp;
use anyhow::{Result, anyhow, ensure};
use std::io::prelude::*;
use std::net::TcpStream;
use tracing::*;

#[derive(Debug)]
pub struct ClientConfig {
    pub cursor: Option<Timestamp>,
    pub wanted_collections: Vec<String>,
    pub wanted_dids: Vec<String>,
    pub max_message_size_bytes: usize,
    pub compress: bool,
    pub require_hello: bool,
}

impl ClientConfig {
    fn from_query_params(params: &str) -> anyhow::Result<Self> {
        let mut config = Self {
            cursor: None,
            wanted_collections: vec![],
            wanted_dids: vec![],
            max_message_size_bytes: usize::MAX,
            compress: false,
            require_hello: false,
        };
        for query in params.split('&').filter(|x| !x.is_empty()) {
            let (key, val) = query.split_once('=').unwrap_or((query, ""));
            match key {
                "cursor" => config.cursor = Some(Timestamp(val.parse()?)),
                "wantedCollections" => config.wanted_collections.push(val.to_owned()),
                "wantedDids" => config.wanted_dids.push(val.to_owned()),
                "maxMessageSizeBytes" => {
                    config.max_message_size_bytes = config.max_message_size_bytes.min(val.parse()?)
                }
                "compress" => config.compress = true,
                "requireHello" => config.require_hello = true,
                _ => warn!("Unknown query param: {key}"),
            }
        }
        Ok(config)
    }
}

// TODO: timeout
pub fn perform_handshake(conn: &mut TcpStream) -> Result<ClientConfig> {
    let mut buf = [0; 4096];
    let mut n = 0;
    loop {
        n += conn.read(&mut buf[n..])?;
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut req = httparse::Request::new(&mut headers);
        let status = req.parse(&buf[..n])?;

        match status {
            httparse::Status::Complete(_) => {
                let (key, query_params) = validate_request(req)?;
                send_response(conn, key)?;
                return ClientConfig::from_query_params(query_params);
            }
            httparse::Status::Partial => (), // loop
        }
    }
}

fn validate_request<'b>(req: httparse::Request<'_, 'b>) -> anyhow::Result<(&'b [u8], &'b str)> {
    ensure!(
        req.method == Some("GET"),
        "Bad websocket handshake: wrong method"
    );

    let path = req.path.ok_or(anyhow!("No path"))?;
    let (file_name, query_params) = path.split_once('?').unwrap_or((path, ""));
    ensure!(file_name == "/subscribe");

    let header = |name: &str| -> Result<&[u8]> {
        req.headers
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
    ensure!(
        header("sec-websocket-version")?.eq_ignore_ascii_case(b"13"),
        "Wrong websocket version header"
    );
    let key = header("sec-websocket-key")?;

    Ok((key, query_params))
}

fn send_response(conn: &mut TcpStream, key: &[u8]) -> anyhow::Result<()> {
    let accept = {
        use base64::prelude::*;
        let magic = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
        let mut buf = Vec::with_capacity(key.len() + magic.len());
        buf.extend(key);
        buf.extend(magic);
        BASE64_STANDARD.encode(sha1_smol::Sha1::from(buf).digest().bytes())
    };

    writeln!(conn, "HTTP/1.1 101 Switching Protocols\r")?;
    writeln!(conn, "Connection: Upgrade\r")?;
    writeln!(conn, "Upgrade: websocket\r")?;
    writeln!(conn, "Server: tailsrv\r")?;
    writeln!(conn, "Sec-WebSocket-Accept: {accept}\r")?;
    writeln!(conn, "\r")?;

    Ok(())
}
