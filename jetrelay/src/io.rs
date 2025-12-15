use crate::{
    BUFFER,
    client::{ClientId, ClientWithPipe},
};
use anyhow::{Result, bail, ensure};
use io_uring::{cqueue, opcode, squeue};
use rustix::{fd::AsRawFd, pipe::SpliceFlags};
use slab::Slab;
use std::io::ErrorKind;
use tracing::*;

/// A kind of cookie which you can attach to io_uring submissions, which allows
/// you to match them up with their completions.  The io_uring API requires them
/// to be encoded as a u64.
#[derive(Debug, PartialEq)]
enum UserData {
    FillPipe(ClientId),
    DrainPipe(ClientId),
}

impl From<UserData> for u64 {
    fn from(value: UserData) -> Self {
        #[allow(clippy::identity_op)]
        match value {
            UserData::FillPipe(id) => (0 << 32) | id as u64,
            UserData::DrainPipe(id) => (1 << 32) | id as u64,
        }
    }
}

impl TryFrom<u64> for UserData {
    type Error = anyhow::Error;
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        match value >> 32 {
            0 => Ok(UserData::FillPipe(value as u32)),
            1 => Ok(UserData::DrainPipe(value as u32)),
            x => bail!("{value:x}: Unknown user data: {x}"),
        }
    }
}

fn fill_pipe(client_id: ClientId, client: &mut ClientWithPipe, len: u32) -> squeue::Entry {
    let fd_in = io_uring::types::Fixed(0);
    let fd_out = io_uring::types::Fd(client.pipe_wtr.as_raw_fd());
    let off_in = i64::try_from(client.inner.offset).unwrap();
    let off_out = -1; // Pipes don't have offsets
    opcode::Splice::new(fd_in, off_in, fd_out, off_out, len)
        .flags(SpliceFlags::NONBLOCK.bits())
        .build()
        .user_data(UserData::FillPipe(client_id).into())
}

fn drain_pipe(client_id: ClientId, client: &mut ClientWithPipe) -> squeue::Entry {
    let fd_in = io_uring::types::Fd(client.pipe_rdr.as_raw_fd());
    let fd_out = io_uring::types::Fd(client.inner.conn.as_raw_fd());
    let off_in = -1; // Pipes don't have offsets
    let off_out = -1; // Sockets don't have offsets
    let len = u32::MAX; // As much as possible (note: the op will return an error)
    opcode::Splice::new(fd_in, off_in, fd_out, off_out, len)
        .flags(SpliceFlags::NONBLOCK.bits())
        .build()
        .user_data(UserData::DrainPipe(client_id).into())
}

/// Issue IOs for a single client
///
/// ## Why fill and drain a pipe?
///
/// io_uring doesn't have a sendfile() opcode.  However, we can emulate it
/// by splicing once from the file to a pipe, and then again from the pipe to
/// the socket.  This is exactly how sendfile() works under the hood, so there
/// should be no performance impact from this.
///
/// ## Back-pressure
///
/// If the client is not reading their socket, eventually the socket buffer
/// on our side will fill up.  At this point, the next `DrainPipe` will block
/// and the bytes will remain in the pipe.  `send_in_flight` will be stuck at
/// `true`, preventing any more `DrainPipe`s from being submitted.
///
/// We'll keep submitting `FillPipe`s until the pipe's buffer fills up. At this
/// point, the next `FillPipe` will block.  `copy_in_flight` be stuck at `true`,
/// preventing any more `FillPipe`s from being submitted.
pub fn get_client_caught_up(
    sqes: &mut Vec<squeue::Entry>,
    file_len: u64,
    client_id: ClientId,
    client: &mut ClientWithPipe,
) -> Result<()> {
    let _g = debug_span!("", client_id).entered();
    let n_pages = file_len / BUFFER;
    let sent_pages = client.inner.offset / BUFFER;
    if !client.copy_in_flight && sent_pages < n_pages {
        let n_bytes = u32::try_from((n_pages - sent_pages) * BUFFER).unwrap();
        debug!("Copying {n_bytes} bytes into the pipe");
        sqes.push(fill_pipe(client_id, client, n_bytes));
        client.copy_in_flight = true;
    }
    if !client.send_in_flight && client.bytes_in_pipe > 0 {
        debug!("Sending {} bytes to the socket", client.bytes_in_pipe);
        sqes.push(drain_pipe(client_id, client));
        client.send_in_flight = true;
    }
    Ok(())
}

pub fn handle_completion(clients: &mut Slab<ClientWithPipe>, cqe: cqueue::Entry) -> Result<()> {
    let user_data = UserData::try_from(cqe.user_data())?;
    let result = cqe.result();
    debug!("{user_data:?} completed with {result:?}");
    let client_id = match user_data {
        UserData::FillPipe(client_id) => client_id,
        UserData::DrainPipe(client_id) => client_id,
    };
    let _g = info_span!("", client_id).entered();
    let bytes_written = match result {
        Ok(x) => x as u64,
        Err(e)
            if matches!(e.kind(), ErrorKind::BrokenPipe | ErrorKind::ConnectionReset)
                || e.raw_os_error() == Some(9) =>
        {
            match user_data {
                UserData::FillPipe(_) => {
                    // This happens when the client is gone
                    assert!(clients.get_mut(client_id as usize).is_none());
                }
                UserData::DrainPipe(_) => {
                    info!("Socket closed by other side");
                    let client = clients.try_remove(client_id as usize);
                    ensure!(client.is_some(), "Two hangups for the same client?");
                }
            }
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let Some(client) = clients.get_mut(client_id as usize) else {
        warn!("Got an IO completion but the client is gone");
        return Ok(());
    };
    match user_data {
        UserData::FillPipe(_) => {
            ensure!(client.copy_in_flight);
            client.copy_in_flight = false;
            ensure!(bytes_written != 0);
            client.bytes_in_pipe += bytes_written;
            client.inner.offset += bytes_written;
        }
        UserData::DrainPipe(_) => {
            ensure!(client.send_in_flight);
            client.send_in_flight = false;
            ensure!(bytes_written != 0);
            debug!("Sent {bytes_written} bytes to client");
            client.bytes_in_pipe -= bytes_written;
        }
    }
    Ok(())
}
