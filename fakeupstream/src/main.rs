use std::{
    io::Write,
    net::{SocketAddr, TcpListener},
    time::Duration,
};
use wsclient::Frame;
use wsserver::Timestamp;

const PORT: u16 = 7376;
const TARGET_HZ: usize = 400;

fn main() -> anyhow::Result<()> {
    let mut txt = format!(
        "{{ \"time_us\": {:0>16}, \"padding\": \"{:>466}\" }}",
        0, ' '
    )
    .into_bytes();

    println!("Ev size: {} B", txt.len());
    println!("target HZ = {TARGET_HZ}");
    let rate = (TARGET_HZ * txt.len()) as f32 / 1024.;
    println!("Bitrate: {:.1} KiB/s = {:.1} Mbps", rate, rate * 8. / 1024.);

    let listen_addr = SocketAddr::new([0, 0, 0, 0].into(), PORT);
    let listener = TcpListener::bind(listen_addr)?;

    loop {
        let (mut conn, addr) = listener.accept()?;
        println!("addr={addr:?}");
        // let config = wsserver::perform_handshake(&mut conn)?;
        // println!("config={config:?}");

        let mut prev = Timestamp::now().0 / 1_000_000;
        let mut frame = Frame::text(std::str::from_utf8(&txt).unwrap());
        let mut sent = 0;
        loop {
            if sent >= TARGET_HZ {
                let sleep = (prev + 1) * 1_000_000 - Timestamp::now().0;
                std::thread::sleep(Duration::from_micros(sleep));
            }
            let ts = Timestamp::now();

            let this = ts.0 / 1_000_000;
            if this != prev {
                sent = 0;
                let mut cursor = std::io::Cursor::new(&mut txt[13..]);
                write!(cursor, "{:>16}", ts.0)?;
                frame = Frame::text(std::str::from_utf8(&txt)?);
            }
            prev = this;

            sent += 1;
            match conn.write_all(&frame.bytes) {
                Ok(_) => (),
                Err(_) => {
                    println!("Disconnected");
                    break;
                }
            }
        }
    }
}
