use anyhow::ensure;
use bpaf::{Bpaf, Parser};
use jiff::{Span, Timestamp};
use std::{
    num::NonZero,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
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

struct WorkerState {
    timestamp: AtomicU64,
    count: AtomicU64,
}

impl Default for WorkerState {
    fn default() -> Self {
        WorkerState {
            timestamp: AtomicU64::new(u64::MAX),
            count: AtomicU64::new(0),
        }
    }
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
    let states: Vec<_> = std::iter::repeat_with(|| WorkerState::default())
        .take(opts.jobs.into())
        .collect();
    let start = Instant::now();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for x in &states {
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
            let mut total_count = 0;
            let mut n = 0;
            for x in &states {
                let ts = x.timestamp.load(Ordering::Acquire);
                oldest_ts = oldest_ts.min(ts);
                total_count += x.count.load(Ordering::Acquire);
                if ts != u64::MAX {
                    n += 1;
                }
            }
            let d = start.elapsed();
            if oldest_ts == u64::MAX {
                println!("Worst lag [{n}]: -- no data --");
            } else {
                let rate = total_count as f64 / d.as_secs_f64() / n as f64;
                let oldest_ts = Timestamp::from_microsecond(oldest_ts as i64).unwrap();
                let worst_lag = Timestamp::now().duration_since(oldest_ts);
                println!("Worst lag [{n}]: {worst_lag:?} ({rate:.0} ev/s)");
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn worker(url: &Url, x: &WorkerState) -> anyhow::Result<()> {
    for frame in wsclient::connect_websocket(url)? {
        let frame = frame?;
        ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
        ensure!(frame.mask().is_none(), "Frame is masked");
        ensure!(frame.opcode() == wsclient::OpCode::Text, "Non-text frame");
        let payload = std::str::from_utf8(frame.payload())?;
        let timestamp = gjson::get(payload, "time_us").u64();
        x.timestamp.store(timestamp, Ordering::Release);
        x.count.fetch_add(1, Ordering::Release);
    }
    Ok(())
}
