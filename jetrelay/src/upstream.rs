use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::*;
use wsclient::Frame;

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
        let frame = frame?;
        ensure!(frame.reserved_bits() == 0, "Non-zero reserved bits");
        ensure!(frame.mask().is_none(), "Frame is masked");
        ensure!(frame.opcode() == wsclient::OpCode::Text, "Non-text frame");
        file.write_all(&frame.bytes).unwrap();
        file.flush().unwrap();
        let n = frame.bytes.len() as u64;
        trace!("Wrote {n} bytes");
        let offset = file_len.fetch_add(n, Ordering::Release);

        let payload = std::str::from_utf8(frame.payload())?;
        let timestamp = Timestamp(gjson::get(payload, "time_us").u64());
        INDEX.lock().unwrap().insert(timestamp, offset);
        // We could wake up the io_uring here... but we don't bother
    }
    Ok(())
}
