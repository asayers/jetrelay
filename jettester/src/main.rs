use anyhow::Context;
use bpaf::{Bpaf, Parser};
use futures::{Stream, StreamExt, TryStreamExt};
use jiff::{Span, Timestamp};
use rlimit::Resource;
use std::{
    collections::BTreeMap,
    num::NonZero,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio_tungstenite::tungstenite::Message;
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
// static N_MSGS: AtomicUsize = AtomicUsize::new(0);
static N_BYTES: AtomicUsize = AtomicUsize::new(0);
static WORST_CLIENT_0: AtomicUsize = AtomicUsize::new(usize::MAX);
static WORST_CLIENT_1: AtomicUsize = AtomicUsize::new(usize::MAX);
static WORST_CLIENT_2: AtomicUsize = AtomicUsize::new(usize::MAX);
static WORST_CLIENT_3: AtomicUsize = AtomicUsize::new(usize::MAX);

static FOO: Mutex<Foo> = Mutex::new(Foo {
    new_msgs: 0,
    new_bytes: 0,
    oldest_ts: u64::MAX,
    global_stats: BTreeMap::new(),
    mb_total: 0.,
    secs_total: 0.,
});

struct Foo {
    new_msgs: usize,
    new_bytes: usize,
    oldest_ts: u64,
    global_stats: BTreeMap<u64, (usize, MsgStats)>,
    mb_total: f32,
    secs_total: f32,
}

impl Foo {
    fn merge(&mut self, x: SecondStats) {
        self.new_msgs += x.stats.n_msgs;
        self.new_bytes += x.stats.n_bytes;
        self.oldest_ts = self.oldest_ts.min(x.second);
        let (n, stats) = self.global_stats.entry(x.second).or_insert((0, x.stats));
        assert_eq!(*stats, x.stats);
        *n += 1;
    }
}

#[tokio::main]
pub async fn main() {
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

    let lim = opts.jobs.get() as u64 + 1024;
    rlimit::setrlimit(Resource::NOFILE, lim, lim * 2).unwrap();

    // let start = Instant::now();
    tokio::task::spawn(async move {
        for id in 0..opts.jobs.into() {
            // let slow = i % 2 == 0;
            let slow = false;
            let url = url.to_string();
            tokio::task::spawn(async move {
                for _ in 0..opts.retries {
                    let (upstream, _) = match tokio_tungstenite::connect_async(&url).await {
                        Ok(x) => x,
                        Err(e) => {
                            eprintln!("Connection error: {e:#}");
                            continue;
                        }
                    };
                    N_CONNECTED.fetch_add(1, Ordering::Release);
                    match worker(
                        upstream.map_err(|e| anyhow::anyhow!(e)),
                        opts.dump,
                        slow,
                        id,
                    )
                    .await
                    {
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
    let start = Instant::now();
    let mut prev_mibytes = 0.;
    let mut avg = 0.;
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

        // let mut foo = FOO.lock().unwrap();

        // for second in (foo.oldest_ts - 1)..(Timestamp::now().as_second() as u64) {
        //     let d = second as i64 - Timestamp::now().as_second();
        //     if let Some((n, stats)) = foo.global_stats.get(&second) {
        //         println!(
        //             "[{second}] T{d:+}s ({} evs, {} KiB) x{n}",
        //             // stats.hash,
        //             stats.n_msgs,
        //             stats.n_bytes / 1024,
        //         );
        //     } else {
        //         println!("[{second}] T{d:+}s (??? evs, ??? KiB) x0");
        //     }
        // }
        // let mb = foo.new_bytes as f32 / 1024. / 1024.;
        let n_clients = N_CONNECTED.load(Ordering::Acquire);
        // let n_msgs = N_MSGS.load(Ordering::Acquire);
        let n_mibytes = N_BYTES.load(Ordering::Acquire) as f32 / 1024. / 1024.;
        let worst_client_0 =
            Timestamp::from_microsecond(WORST_CLIENT_0.load(Ordering::Acquire) as i64).unwrap();
        let worst_client_1 =
            Timestamp::from_microsecond(WORST_CLIENT_1.load(Ordering::Acquire) as i64).unwrap();
        let worst_client_2 =
            Timestamp::from_microsecond(WORST_CLIENT_2.load(Ordering::Acquire) as i64).unwrap();
        let worst_client_3 =
            Timestamp::from_microsecond(WORST_CLIENT_3.load(Ordering::Acquire) as i64).unwrap();
        let worst_client = worst_client_0
            .min(worst_client_1)
            .min(worst_client_2)
            .min(worst_client_3);
        WORST_CLIENT_0.store(usize::MAX, Ordering::Release);
        WORST_CLIENT_1.store(usize::MAX, Ordering::Release);
        WORST_CLIENT_2.store(usize::MAX, Ordering::Release);
        WORST_CLIENT_3.store(usize::MAX, Ordering::Release);
        let rate = (n_mibytes - prev_mibytes) * 8. / 1024.;
        prev_mibytes = n_mibytes;
        avg += rate;
        avg /= 2.0;
        println!(
            "{:.2}/{:.2} Gbps ({:.2} avg) ({n_clients} connected), worst: {worst_client} ({:.1}s, {:.1}s/{:.1}s/{:.1}s/{:.1}s)",
            rate,
            1.56 * n_clients as f32 / 1024.,
            avg,
            (Timestamp::now() - worst_client).get_seconds(),
            (Timestamp::now() - worst_client_0).get_seconds(),
            (Timestamp::now() - worst_client_1).get_seconds(),
            (Timestamp::now() - worst_client_2).get_seconds(),
            (Timestamp::now() - worst_client_3).get_seconds(),
        );
        // println!(
        //     "Total this second: {} evs, {:.2} MiB = {} KHz, {:.2} Gbps, {:.2} Mbps/client ({} connected)",
        //     foo.new_msgs,
        //     mb,
        //     foo.new_msgs / 1000,
        //     mb * 8. / 1024.,
        //     if n_clients == 0 {
        //         0.
        //     } else {
        //         mb * 8. / n_clients as f32
        //     },
        //     n_clients,
        // );
        // let secs = start.elapsed().as_secs() as usize;
        // if secs > 15 {
        //     foo.mb_total += mb;
        //     foo.secs_total += 1.;
        //     println!(
        //         "Average so far: {} Gbps ({}s)",
        //         foo.mb_total * 8. / 1024. / foo.secs_total,
        //         foo.secs_total,
        //     );
        // }
        // println!();

        // foo.new_msgs = 0;
        // foo.new_bytes = 0;
        // foo.oldest_ts = u64::MAX;
        // std::mem::drop(foo);
        std::thread::sleep(Duration::from_secs(1));
    }
}

async fn worker(
    mut iter: impl Stream<Item = anyhow::Result<Message>> + Unpin,
    dump: bool,
    slow: bool,
    id: usize,
) -> anyhow::Result<()> {
    let mut warming_up = 0;
    let mut last_ts_sec = 0;
    // let mut stats = MsgStats::default();
    while let Some(frame) = iter.next().await {
        let frame = frame?;
        // ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
        // ensure!(frame.mask().is_none(), "Frame is masked");
        // frame.check_len();
        // match frame.opcode() {
        //     wsclient::OpCode::Text => (),
        //     wsclient::OpCode::Ping => continue, // Ignore
        //     opcode => {
        //         eprintln!("Unexpected opcode: {opcode:?}");
        //         continue;
        //     }
        // }
        // let payload = std::str::from_utf8(frame.payload())?;
        let payload = match frame {
            Message::Text(payload) => payload,
            Message::Ping(_) => continue, // Ignore
            _ => {
                eprintln!("Unexpected opcode: {frame:?}");
                continue;
            }
        };

        let timestamp: u64 = payload
            .strip_prefix(r#"{ "time_us": "#)
            .context("1")
            .and_then(|x| {
                x.strip_suffix(r#"" }"#)
                    .context("2")?
                    .trim_end()
                    .strip_suffix(r#", "padding": ""#)
                    .context("3")
            })?
            .parse()?;

        // let timestamp = gjson::get(payload, "time_us").u64();

        if dump {
            let collection = gjson::get(&payload, "commit.collection");
            let text = gjson::get(&payload, "commit.record.text");
            let timestamp = Timestamp::from_microsecond(timestamp as i64).unwrap();
            println!(
                "[{timestamp}] {collection} ({} bytes) {text}",
                payload.len()
            );
        }

        // let ts_sec = timestamp / 1_000_000;
        // if ts_sec != last_ts_sec {
        //     let stats = SecondStats {
        //         second: last_ts_sec,
        //         stats: std::mem::take(&mut stats),
        //     };
        //     if warming_up < 3 {
        //         warming_up += 1;
        //     } else {
        //         FOO.lock().unwrap().merge(stats);
        //     }
        //     last_ts_sec = ts_sec;
        // }
        // stats.n_bytes += payload.len() + 2; // 2 bytes for the header - just a guess
        // stats.n_msgs += 1;
        N_BYTES.fetch_add(payload.len() + 2, Ordering::AcqRel); // 2 bytes for the header - just a guess
        // N_MSGS.fetch_add(1, Ordering::AcqRel);
        match id % 4 {
            0 => &WORST_CLIENT_0,
            1 => &WORST_CLIENT_1,
            2 => &WORST_CLIENT_2,
            3 => &WORST_CLIENT_3,
            _ => unreachable!(),
        }
        .fetch_min(timestamp as usize, Ordering::AcqRel);
        // for bytes in frame.payload().chunks_exact(8) {
        //     stats.hash ^= u64::from_le_bytes(bytes.try_into().unwrap());
        // }
        if slow {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    Ok(())
}
