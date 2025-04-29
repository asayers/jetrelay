use anyhow::ensure;
use bpaf::{Bpaf, Parser};
use jiff::{Span, Timestamp};
use std::{
    collections::BTreeMap,
    num::NonZero,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
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

struct WorkerState {
    stats: mpsc::Sender<SecondStats>,
    // timestamp: AtomicU64,
    // count: AtomicU64,
    // bytes: AtomicU64,
}

// impl Default for WorkerState {
//     fn default() -> Self {
//         WorkerState {
//             stats: boxcar::Vec::new(),
//             // timestamp: AtomicU64::new(u64::MAX),
//             // count: AtomicU64::new(0),
//             // bytes: AtomicU64::new(0),
//         }
//     }
// }

#[derive(Debug)]
struct SecondStats {
    second: u64,
    stats: MsgStats,
}

// impl SecondStats {
//     fn ts(&self) -> Timestamp {
//         Timestamp::from_second(self.second as i64).unwrap()
//     }
// }

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
struct MsgStats {
    n_msgs: usize,
    n_bytes: usize,
    hash: u64,
}

static N_CONNECTED: AtomicUsize = AtomicUsize::new(0);

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
    let (tx, rx) = mpsc::channel();
    let states: Vec<_> = std::iter::repeat_with(|| WorkerState { stats: tx.clone() })
        .take(opts.jobs.into())
        .collect();

    // let start = Instant::now();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for x in &states {
                scope.spawn(|| {
                    for _ in 0..opts.retries {
                        let iter = match wsclient::connect_websocket(&url) {
                            Ok(x) => x,
                            Err(e) => {
                                eprintln!("Connection error: {e:#}");
                                continue;
                            }
                        };
                        N_CONNECTED.fetch_add(1, Ordering::Release);
                        match worker(iter, x) {
                            Ok(()) => {
                                eprintln!("Connection closed by server");
                                break;
                            }
                            Err(e) => eprintln!("Error: {e:#}"),
                        }
                        N_CONNECTED.fetch_sub(1, Ordering::Release);
                        std::thread::sleep(Duration::from_millis(opts.wait));
                    }
                });
                std::thread::sleep(Duration::from_millis(opts.wait));
            }
        });
        // let mut global_stats = BTreeMap::<u64, (usize, MsgStats)>::new();
        let mut global_stats = BTreeMap::<u64, BTreeMap<MsgStats, usize>>::new();
        loop {
            // let mut oldest_ts = u64::MAX;
            // let mut total_count = 0;
            // let mut total_bytes = 0;
            // let n_connected = N_CONNECTED.load(Ordering::Acquire);
            // for x in &states {
            //     let stats = x.stats.get(x.stats.count()).unwrap();
            //     match global_stats.entry(stats.second) {
            //         Entry::Vacant(vacant_entry) => {
            //             vacant_entry.insert((1, stats.stats));
            //         }
            //         Entry::Occupied(occupied_entry) => {
            //             let (n, expected) = occupied_entry.get_mut();
            //             assert_eq!(*expected, stats.stats);
            //             *n += 1;
            //             if *n == n_connected {
            //                 occupied_entry.remove();
            //                 eprintln!();
            //             }
            //         }
            //     }
            //     // .and_modify(|(n, existing)| {
            //     // })
            //     let ts = stats.second;
            //     oldest_ts = oldest_ts.min(ts);
            //     global_stats.total_count += x.count.load(Ordering::Acquire);
            //     total_bytes += x.bytes.load(Ordering::Acquire);
            //     if ts != u64::MAX {}
            // }
            // let d = start.elapsed();
            // if oldest_ts == u64::MAX {
            //     println!("Worst lag [{n_connected}]: -- no data --");
            // } else {
            //     let rate = total_count as f64 / d.as_secs_f64() / n_connected as f64;
            //     let rate2 = total_bytes as f64 / d.as_secs_f64() / 1024. / 1024.;
            //     let oldest_ts = Timestamp::from_microsecond(oldest_ts as i64).unwrap();
            //     let worst_lag = Timestamp::now().duration_since(oldest_ts);
            //     println!(
            //         "Worst lag [{n_connected}]: {worst_lag:?} ({rate:.0} ev/s, {rate2:.2} MiB/s)"
            //     );
            // }

            // while let Ok(x) = rx.try_recv() {
            //     println!("{x:?}");
            // }
            //

            let mut new_msgs = 0;
            let mut new_bytes = 0;
            let mut oldest_ts = u64::MAX;
            while let Ok(x) = rx.try_recv() {
                new_msgs += x.stats.n_msgs;
                new_bytes += x.stats.n_bytes;
                oldest_ts = oldest_ts.min(x.second);
                let second_stats = global_stats.entry(x.second).or_default();
                let n = second_stats.entry(x.stats).or_default();
                *n += 1;
            }

            for (second, xs) in global_stats.range(oldest_ts..) {
                let d = *second as i64 - Timestamp::now().as_second();
                for (stats, n) in xs {
                    println!(
                        "T{d:+}s {:#x} x{n} ({} evs, {} KiB)",
                        stats.hash,
                        stats.n_msgs,
                        stats.n_bytes / 1024,
                    );
                }
            }
            let mb = new_bytes / 1024 / 1024;
            println!("Total: {} evs, {} MiB = {} Mbps", new_msgs, mb, mb * 8);
            println!();

            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn worker(
    iter: impl Iterator<Item = std::io::Result<wsclient::Frame>>,
    x: &WorkerState,
) -> anyhow::Result<()> {
    let mut warming_up = 0;
    let mut last_ts_sec = 0;
    let mut stats = MsgStats::default();
    for frame in iter {
        let frame = frame?;
        ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
        ensure!(frame.mask().is_none(), "Frame is masked");
        match frame.opcode() {
            wsclient::OpCode::Text => (),
            wsclient::OpCode::Ping => continue, // Ignore
            opcode => {
                eprintln!("Unexpected opcode: {opcode:?}");
                continue;
            }
        }
        let payload = std::str::from_utf8(frame.payload())?;
        let timestamp = gjson::get(payload, "time_us").u64();

        let ts_sec = timestamp / 1_000_000;
        if ts_sec != last_ts_sec {
            let stats = SecondStats {
                second: last_ts_sec,
                stats: std::mem::take(&mut stats),
            };
            if warming_up < 3 {
                warming_up += 1;
            } else {
                x.stats.send(stats).unwrap();
            }
            last_ts_sec = ts_sec;
        }
        stats.n_bytes += frame.bytes.len();
        stats.n_msgs += 1;
        // for bytes in frame.payload().chunks_exact(8) {
        //     stats.hash ^= u64::from_le_bytes(bytes.try_into().unwrap());
        // }
    }
    Ok(())
}
