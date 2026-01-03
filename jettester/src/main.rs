use anyhow::{Context, ensure};
use bpaf::{Bpaf, Parser};
use jiff::{Span, Timestamp};
use rlimit::Resource;
use std::{
    collections::BTreeMap,
    num::NonZero,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
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
    dump: bool,
    #[bpaf(positional("URL"))]
    url: Url,
}

#[derive(Clone)]
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

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct MsgStats {
    n_msgs: usize,
    n_bytes: usize,
    // hash: u64,
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
    let states: Vec<_> =
        std::iter::repeat_n(WorkerState { stats: tx.clone() }, opts.jobs.into()).collect();

    let lim = opts.jobs.get() as u64 + 1024;
    rlimit::setrlimit(Resource::NOFILE, lim, lim * 2).unwrap();

    // let start = Instant::now();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for x in &states {
                // let slow = i % 2 == 0;
                let slow = false;
                let url = &url;
                scope.spawn(move || {
                    for _ in 0..opts.retries {
                        let iter = match wsclient::connect_websocket(url) {
                            Ok(x) => x,
                            Err(e) => {
                                eprintln!("Connection error: {e:#}");
                                continue;
                            }
                        };
                        N_CONNECTED.fetch_add(1, Ordering::Release);
                        match worker(iter, x, opts.dump, slow) {
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
        let mut global_stats = BTreeMap::<u64, (usize, MsgStats)>::new();
        let mut mb_total = 0.;
        let mut secs_total = 0.;
        let start = Instant::now();
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
                let (n, stats) = global_stats.entry(x.second).or_insert((0, x.stats));
                assert_eq!(*stats, x.stats);
                *n += 1;
            }

            for second in (oldest_ts - 1)..(Timestamp::now().as_second() as u64) {
                let d = second as i64 - Timestamp::now().as_second();
                if let Some((n, stats)) = global_stats.get(&second) {
                    println!(
                        "[{second}] T{d:+}s ({} evs, {} KiB) x{n}",
                        // stats.hash,
                        stats.n_msgs,
                        stats.n_bytes / 1024,
                    );
                } else {
                    println!("[{second}] T{d:+}s (??? evs, ??? KiB) x0");
                }
            }
            let mb = new_bytes as f32 / 1024. / 1024.;
            let n_clients = N_CONNECTED.load(Ordering::Acquire);
            println!(
                "Total this second: {} evs, {:.2} MiB = {} KHz, {:.2} Gbps, {:.2} Mbps/client ({} connected)",
                new_msgs,
                mb,
                new_msgs / 1000,
                mb * 8. / 1024.,
                if n_clients == 0 {
                    0.
                } else {
                    mb * 8. / n_clients as f32
                },
                n_clients,
            );
            let secs = start.elapsed().as_secs() as usize;
            if secs > 15 {
                mb_total += mb;
                secs_total += 1.;
                println!(
                    "Average so far: {} Gbps ({secs_total}s)",
                    mb_total * 8. / 1024. / secs_total,
                );
            }
            println!();

            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn worker(
    iter: impl Iterator<Item = std::io::Result<wsclient::Frame>>,
    x: &WorkerState,
    dump: bool,
    slow: bool,
) -> anyhow::Result<()> {
    let mut warming_up = 0;
    let mut last_ts_sec = 0;
    let mut stats = MsgStats::default();
    for frame in iter {
        let frame = frame?;
        ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
        ensure!(frame.mask().is_none(), "Frame is masked");
        frame.check_len();
        match frame.opcode() {
            wsclient::OpCode::Text => (),
            wsclient::OpCode::Ping => continue, // Ignore
            opcode => {
                eprintln!("Unexpected opcode: {opcode:?}");
                continue;
            }
        }
        let payload = std::str::from_utf8(frame.payload())?;

        let timestamp: u64 = payload
            .strip_prefix(r#"{ "time_us": "#)
            .context("1")
            .and_then(|x| {
                x.strip_suffix(r#"" }"#)
                    .context("2")?
                    .trim_end()
                    .strip_suffix(r#", "padding": ""#)
                    .context("3")
            })
            .with_context(|| format!("{frame:?}"))?
            .parse()?;

        // let timestamp = gjson::get(payload, "time_us").u64();

        if dump {
            let collection = gjson::get(payload, "commit.collection");
            let text = gjson::get(payload, "commit.record.text");
            let timestamp = Timestamp::from_microsecond(timestamp as i64).unwrap();
            println!(
                "[{timestamp}] {collection} ({} bytes) {text}",
                payload.len()
            );
        }

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
        if slow {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}
