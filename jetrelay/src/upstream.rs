use anyhow::{Context, Result, bail, ensure};
use rustix::fs::FallocateFlags;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::*;
use wsclient::{Frame, OpCode};
use wsserver::Timestamp;

pub fn connect_to_upstream() -> anyhow::Result<Receiver<(Frame, Timestamp)>> {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let var = "UPSTREAM_URL";
    match std::env::var(var) {
        Ok(url) => {
            let url = url.parse().context(var)?;
            let frames = wsclient::connect_websocket(&url)?;
            std::thread::Builder::new()
                .name("event_recv".to_owned())
                .spawn(|| crate::upstream::jetstream_iter(frames, event_tx))?
        }
        Err(_) => panic!(),
        // std::thread::Builder::new()
        //     .name("event_recv".to_owned())
        //     .spawn(|| fake_iter(event_tx))?,
    };
    info!("Connected to upstream");
    Ok(event_rx)
}

// pub fn fake_iter(event_tx: Sender<(Frame, Timestamp)>) -> anyhow::Result<()> {
//     let mut txt = format!(
//         "{{ \"time_us\": {:0>16}, \"padding\": \"{:>500}\" }}",
//         0, ' '
//     )
//     .into_bytes();
//     // let mut txt = format!(
//     //     "{{ \"time_us\": {:0>16}, \"padding\": \"{:>65000}\" }}",
//     //     0, ' '
//     // )
//     // .into_bytes();
//     let mut prev = Timestamp::now().0 / 1_000_000;
//     let target = 400 /* Hz */;
//     let mut frame = Frame::text(std::str::from_utf8(&txt).unwrap());
//     let mut sent = 0;
//     loop {
//         if sent >= target {
//             let sleep = (prev + 1) * 1_000_000 - Timestamp::now().0;
//             std::thread::sleep(Duration::from_micros(sleep));
//         }
//         let ts = Timestamp::now();

//         let this = ts.0 / 1_000_000;
//         if this != prev {
//             sent = 0;
//             let mut cursor = std::io::Cursor::new(&mut txt[13..]);
//             write!(cursor, "{:>16}", ts.0)?;
//             frame = Frame::text(std::str::from_utf8(&txt)?);
//         }
//         prev = this;

//         sent += 1;
//         event_tx.send((frame.clone(), ts))?;
//     }
// }

pub fn jetstream_iter(
    ws_iter: impl Iterator<Item = std::io::Result<Frame>>,
    event_tx: Sender<(Frame, Timestamp)>,
) -> anyhow::Result<()> {
    for frame in ws_iter {
        let frame = frame.context("I/O error while reading from websocket")?;
        if let Some(ts) = parse_frame(&frame).with_context(|| format!("{:?}", frame.bytes))? {
            event_tx.send((frame, ts))?;
        }
    }
    Ok(())
}

pub fn mk_stat_printer() -> impl FnMut(&Frame, Timestamp, u64) {
    #[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone)]
    struct MsgStats {
        n_msgs: usize,
        n_bytes: usize,
        // hash: u64,
    }
    let mut last_ts_sec = 0;
    let mut stats = MsgStats::default();
    move |frame: &Frame, timestamp: Timestamp, file_len: u64| {
        let ts_sec = timestamp.0 / 1_000_000;
        if ts_sec != last_ts_sec {
            let stats = std::mem::take(&mut stats);
            info!(
                "[{last_ts_sec}] ({} evs, {} MiB = {} Mbps), {} MiB total",
                stats.n_msgs,
                stats.n_bytes / 1024 / 1024,
                stats.n_bytes * 8 / 1024 / 1024,
                file_len / 1024 / 1024,
            );
            last_ts_sec = ts_sec;
        }
        stats.n_bytes += frame.bytes.len();
        stats.n_msgs += 1;
        // for bytes in frame.payload().chunks_exact(8) {
        //     stats.hash ^= u64::from_le_bytes(bytes.try_into().unwrap());
        // }
    }
}

/// The principle here is to make push as fast as possible.  Searching only
/// needs to be "fast enough".  Therefore we write both timestamps and offsets
/// to a single vector: although separate vectors would be faster to search, a
/// single vector is faster to push to.
pub static INDEX: Mutex<BTreeMap<Timestamp, u64>> = Mutex::new(BTreeMap::new());

pub fn resolve_cursor(ts: Timestamp) -> Option<u64> {
    INDEX.lock().unwrap().range(ts..).next().map(|x| *x.1)
}

const MIN_RETENTION: Duration = Duration::from_secs(60);
const MAX_RETENTION: Duration = Duration::from_secs(2 * 60);

pub fn copy_frames_to_file(
    mut file: File,
    file_len: Arc<AtomicU64>,
    iter: impl IntoIterator<Item = (Frame, Timestamp)>,
) -> Result<()> {
    let _g = info_span!("upstream copier thread").entered();
    info!("Copying data from upstream");
    let mut first_timestamp = Timestamp(0);
    let mut print_stats = crate::upstream::mk_stat_printer();
    for (frame, ts) in iter {
        file.write_all(&frame.bytes)?;
        // file.flush()?;
        let n = frame.bytes.len() as u64;
        trace!("Wrote {n} bytes");
        match handle_frame(&mut first_timestamp, &mut file, &file_len, &frame, ts) {
            Ok(file_len) => print_stats(&frame, ts, file_len),
            Err(e) => warn!("Bad frame: {e:#}"),
        }
    }
    Ok(())
}

pub fn handle_frame(
    first_timestamp: &mut Timestamp,
    file: &mut File,
    file_len: &AtomicU64,
    frame: &Frame,
    timestamp: Timestamp,
) -> Result<u64> {
    file.write_all(&frame.bytes)?;
    // file.flush()?;
    let n = frame.bytes.len() as u64;
    trace!("Wrote {n} bytes");
    let offset = file_len.fetch_add(n, Ordering::Release);

    INDEX.lock().unwrap().insert(timestamp, offset);

    // If retention is over the max, drop until it's at the min
    if *first_timestamp < timestamp - MAX_RETENTION {
        drop_old_data(file, timestamp - MIN_RETENTION)?;
        *first_timestamp = INDEX
            .lock()
            .unwrap()
            .first_key_value()
            .map_or(Timestamp(0), |x| *x.0);
        debug!("Dropped some data, new first_timestamp={first_timestamp:?}");
    }

    // We could wake up the io_uring here... but we don't bother
    Ok(offset)
}

fn parse_frame(frame: &Frame) -> Result<Option<Timestamp>> {
    match frame.opcode() {
        OpCode::Text => (),              // Expected
        OpCode::Ping => return Ok(None), // Ignore
        OpCode::Close => bail!("Upstream is shutting us down :-("),
        OpCode::Binary => bail!("Binary frame: {frame:?}"),
        x => bail!("Unexpected opcode: {x:?}"),
    }
    ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
    ensure!(frame.mask().is_none(), "Frame is masked");
    let payload = std::str::from_utf8(frame.payload())?;
    let timestamp = gjson::get(payload, "time_us");
    ensure!(timestamp.kind() == gjson::Kind::Number);
    let timestamp = Timestamp(timestamp.u64());
    Ok(Some(timestamp))
}

fn drop_old_data(file: &File, ts: Timestamp) -> Result<()> {
    static LAST_DROP_OFFSET: AtomicU64 = AtomicU64::new(0);

    let mut index = INDEX.lock().unwrap();
    let mut x = index.split_off(&ts);
    // split_off() returns everything after `ts`, but we want it the other way round
    std::mem::swap(&mut x, &mut *index);
    std::mem::drop(index);

    if let Some((_, offset)) = x.last_key_value() {
        debug!("Dropping data up to ts={ts:?}, offset={offset}");
        let flags = FallocateFlags::PUNCH_HOLE | FallocateFlags::KEEP_SIZE;
        rustix::fs::fallocate(file, flags, 0, *offset)?;

        let n_dropped = x.len();
        let duration = MAX_RETENTION - MIN_RETENTION; // approximately
        let last_drop_offset = LAST_DROP_OFFSET.swap(*offset, Ordering::AcqRel);
        let n_bytes = *offset - last_drop_offset;
        info!("Over the last {duration:?} we recorded {n_dropped} msgs taking {n_bytes} bytes");
        info!(
            "Rate: {:.0} msgs/s, {:.1} KiB/s",
            n_dropped as f64 / duration.as_secs_f64(),
            n_bytes as f64 / duration.as_secs_f64() / 1024.,
        );
    } else {
        warn!("Tried to drop up to ts={ts:?}, but there's no data that old");
    }
    Ok(())
}
