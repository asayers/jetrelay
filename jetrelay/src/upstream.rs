use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::*;
use wsclient::{Frame, OpCode};

#[derive(Ord, PartialOrd, Eq, PartialEq, Debug, Copy, Clone)]
pub struct Timestamp(pub u64);

/// The principle here is to make push as fast as possible.  Searching only
/// needs to be "fast enough".  Therefore we write both timestamps and offsets
/// to a single vector: although separate vectors would be faster to search, a
/// single vector is faster to push to.
pub static INDEX: Mutex<BTreeMap<Timestamp, u64>> = Mutex::new(BTreeMap::new());

pub fn resolve_cursor(ts: Timestamp) -> Option<u64> {
    INDEX.lock().unwrap().range(ts..).next().map(|x| *x.1)
}

pub fn copy_frames_to_file(
    mut file: File,
    file_len: Arc<AtomicU64>,
    iter: impl Iterator<Item = anyhow::Result<Frame>>,
) -> Result<()> {
    let _g = info_span!("upstream copier thread").entered();
    info!("Copying data from upstream");
    for frame in iter {
        match frame
            .and_then(|frame| handle_frame(&mut file, &file_len, frame))
        {
            Ok(()) => (),
            Err(e) => warn!("{e:#}"),
        }
    }
    Ok(())
}

fn handle_frame(
    file: &mut File,
    file_len: &AtomicU64,
    frame: Frame,
) -> anyhow::Result<()> {
    let timestamp = parse_frame(&frame).with_context(|| format!("{:?}", frame.bytes))?;

    file.write_all(&frame.bytes)?;
    file.flush()?;
    let n = frame.bytes.len() as u64;
    trace!("Wrote {n} bytes");
    let offset = file_len.fetch_add(n, Ordering::Release);

    INDEX.lock().unwrap().insert(timestamp, offset);

    // We could wake up the io_uring here... but we don't bother
    Ok(())
}

fn parse_frame(frame: &Frame) -> anyhow::Result<Timestamp> {
    ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
    ensure!(frame.mask().is_none(), "Frame is masked");
    let opcode = frame.opcode();
    ensure!(opcode == OpCode::Text, "Non-text frame: {opcode:?}",);
    let payload = std::str::from_utf8(frame.payload())?;
    let timestamp = gjson::get(payload, "time_us");
    ensure!(timestamp.kind() == gjson::Kind::Number);
    let timestamp = Timestamp(timestamp.u64());
    Ok(timestamp)
}
