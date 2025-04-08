use anyhow::ensure;
use bpaf::{Bpaf, Parser};
use jiff::{Span, Timestamp};
use std::{
    num::NonZero,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use url::Url;

#[derive(Bpaf)]
struct Opts {
    #[bpaf(long, short, fallback(NonZero::new(1).unwrap()), argument("N"))]
    jobs: NonZero<usize>,
    #[bpaf(long, short, argument("NAME"))]
    collection: Vec<String>,
    #[bpaf(long, short, argument("DURATION"))]
    buffer: Option<Span>,
    #[bpaf(long, short, argument("MILLISECONDS"), fallback(1))]
    wait: u64,
    #[bpaf(long, short, argument("TIMES"), fallback(5))]
    retries: usize,
    #[bpaf(positional("URL"))]
    url: Url,
}

pub fn main() {
    let opts = opts().to_options().run();
    let mut url = opts.url;
    let path = url.path();
    if path != "/subscribe" {
        eprintln!("Bad path: {path}")
    }
    if let Some(x) = opts.buffer {
        let cursor = Timestamp::now() - x;
        eprintln!("Requesting msgs since {cursor}");
        let cursor = cursor.as_microsecond().to_string();
        url.query_pairs_mut().append_pair("cursor", &cursor);
    }
    for c in &opts.collection {
        url.query_pairs_mut().append_pair("wantedCollections", c);
    }
    let timestamps: Vec<_> = std::iter::repeat_with(|| AtomicU64::new(u64::MAX))
        .take(opts.jobs.into())
        .collect();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for x in &timestamps {
                scope.spawn(|| {
                    for _ in 0..opts.retries {
                        match worker(&url, x) {
                            Ok(()) => {
                                eprintln!("Connection closed by server");
                                break;
                            }
                            Err(e) => eprintln!("Error: {e:#}"),
                        }
                        std::thread::sleep(Duration::from_millis(opts.wait));
                    }
                });
                std::thread::sleep(Duration::from_millis(opts.wait));
            }
        });
        loop {
            let mut oldest_ts = u64::MAX;
            let mut n = 0;
            for x in &timestamps {
                let x = x.load(Ordering::Acquire);
                oldest_ts = oldest_ts.min(x);
                if x != u64::MAX {
                    n += 1;
                }
            }
            if oldest_ts == u64::MAX {
                println!("Worst lag [{n}]: -- no data --");
            } else {
                let oldest_ts = Timestamp::from_microsecond(oldest_ts as i64).unwrap();
                let worst_lag = Timestamp::now().duration_since(oldest_ts);
                println!("Worst lag [{n}]: {worst_lag:?}");
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn worker(url: &Url, x: &AtomicU64) -> anyhow::Result<()> {
    for frame in wsclient::connect_websocket(url)? {
        let frame = frame?;
        ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
        ensure!(frame.mask().is_none(), "Frame is masked");
        ensure!(frame.opcode() == wsclient::OpCode::Text, "Non-text frame");
        let payload = std::str::from_utf8(frame.payload())?;
        let timestamp = gjson::get(payload, "time_us").u64();
        x.store(timestamp, Ordering::Release);
    }
    Ok(())
}
