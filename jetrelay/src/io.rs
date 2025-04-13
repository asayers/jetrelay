use crate::{Client, ClientId};
use anyhow::{Result, bail, ensure};
use rustix::fd::AsRawFd;
use rustix::io::Errno;
use rustix::io_uring::io_uring_user_data;
use rustix_uring::{cqueue, opcode, squeue, types::Timespec};
use std::collections::{HashMap, VecDeque};
use tracing::*;

/// A kind of cookie which you can attach to io_uring submissions, which allows
/// you to match them up with their completions.  The io_uring API requires them
/// to be encoded as a u64.
#[derive(Debug, PartialEq)]
enum UserData {
    Timeout,
    FillPipe(ClientId),
    DrainPipe(ClientId),
}

impl From<UserData> for io_uring_user_data {
    fn from(value: UserData) -> Self {
        io_uring_user_data::from_u64(match value {
            UserData::Timeout => 0 << 32,
            UserData::FillPipe(id) => (1 << 32) | id as u64,
            UserData::DrainPipe(id) => (2 << 32) | id as u64,
        })
    }
}

impl TryFrom<io_uring_user_data> for UserData {
    type Error = anyhow::Error;
    fn try_from(value: io_uring_user_data) -> Result<Self, Self::Error> {
        let value = value.u64_();
        match value >> 32 {
            0 => Ok(UserData::Timeout),
            1 => Ok(UserData::FillPipe(value as u32)),
            2 => Ok(UserData::DrainPipe(value as u32)),
            x => bail!("{value:x}: Unknown user data: {x}"),
        }
    }
}

pub fn timeout() -> squeue::Entry {
    const RUNLOOP_TIMEOUT: Timespec = Timespec::new().nsec(100_000_000); // 100 ms
    opcode::Timeout::new(&RUNLOOP_TIMEOUT)
        .build()
        .user_data(UserData::Timeout)
}

fn fill_pipe(client_id: ClientId, client: &mut Client, len: u32) -> squeue::Entry {
    let fd_in = rustix_uring::types::Fixed(0);
    let fd_out = rustix_uring::types::Fd(client.pipe_wtr.as_raw_fd());
    let off_in = i64::try_from(client.offset).unwrap();
    let off_out = -1; // Pipes don't have offsets
    opcode::Splice::new(fd_in, off_in, fd_out, off_out, len)
        .build()
        .user_data(UserData::FillPipe(client_id))
}

fn drain_pipe(client_id: ClientId, client: &mut Client) -> squeue::Entry {
    let fd_in = rustix_uring::types::Fd(client.pipe_rdr.as_raw_fd());
    let fd_out = rustix_uring::types::Fd(client.conn.as_raw_fd());
    let off_in = -1; // Pipes don't have offsets
    let off_out = -1; // Sockets don't have offsets
    let len = u32::MAX; // As much as possible (note: the op will return an error)
    opcode::Splice::new(fd_in, off_in, fd_out, off_out, len)
        .build()
        .user_data(UserData::DrainPipe(client_id))
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
    sqes: &mut VecDeque<squeue::Entry>,
    file_len: u64,
    client_id: ClientId,
    client: &mut Client,
) -> Result<()> {
    let _g = debug_span!("", client_id).entered();
    if !client.copy_in_flight && client.offset < file_len {
        let n_bytes = u32::try_from(file_len - client.offset).unwrap();
        debug!("Copying {n_bytes} bytes into the pipe");
        sqes.push_back(fill_pipe(client_id, client, n_bytes));
        client.copy_in_flight = true;
    }
    if !client.send_in_flight && client.bytes_in_pipe > 0 {
        debug!("Sending {} bytes to the socket", client.bytes_in_pipe);
        sqes.push_back(drain_pipe(client_id, client));
        client.send_in_flight = true;
    }
    Ok(())
}

pub fn handle_completion(
    clients: &mut HashMap<ClientId, Client>,
    cqe: cqueue::Entry,
) -> Result<()> {
    let user_data = UserData::try_from(cqe.user_data())?;
    let result = cqe.result();
    debug!("{user_data:?} completed with {result:?}");
    let (client_id, was_fill) = match user_data {
        UserData::Timeout => return Ok(()),
        UserData::FillPipe(client_id) => (client_id, true),
        UserData::DrainPipe(client_id) => (client_id, false),
    };
    let _g = info_span!("", client_id).entered();
    if matches!(result, Err(Errno::PIPE | Errno::CONNRESET | Errno::BADF)) {
        if was_fill {
            // This happens when the client is gone
            assert!(clients.get_mut(&client_id).is_none());
            return Ok(());
        } else {
            info!("Socket closed by other side");
            let client = clients.remove(&client_id);
            ensure!(client.is_some(), "Two hangups for the same client?");
            return Ok(());
        }
    }
    let Some(client) = clients.get_mut(&client_id) else {
        warn!("Got an IO completion but the client is gone");
        return Ok(());
    };
    let bytes_written = u64::from(result?);
    if was_fill {
        ensure!(client.copy_in_flight);
        client.copy_in_flight = false;
        ensure!(bytes_written != 0);
        client.bytes_in_pipe += bytes_written;
        client.offset += bytes_written;
    } else {
        ensure!(client.send_in_flight);
        client.send_in_flight = false;
        ensure!(bytes_written != 0);
        debug!("Sent {bytes_written} bytes to client");
        client.bytes_in_pipe -= bytes_written;
    }
    Ok(())
}
