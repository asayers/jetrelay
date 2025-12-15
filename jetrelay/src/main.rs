mod client;
mod handshake;
mod impl_1; // thread-per-client, write() blocking
mod impl_2; // vec, write(), nonblocking
mod impl_3; // memfd, sendfile(), nonblocking
mod impl_4; // memfd, splice(), nonblocking
mod impl_5; // memfd, io_uring
mod impl_6; // vec, send_zc(), nonblocking
mod io;
mod upstream;

use anyhow::{Result, bail};
use tracing::Level;
use tracing_subscriber::{EnvFilter, prelude::*};

pub const BUFFER: u64 = 4096;

/// Respects the following env vars:
///
/// * JETRELAY_PORT (required)
/// * UPSTREAM_URL (required)
/// * RUNTIME_DIRECTORY (required)
/// * RUST_LOG
fn main() -> Result<()> {
    log_init();
    match std::env::var("IMPL").as_ref().map(|x| x.as_str()) {
        Ok("1") => crate::impl_1::run(),
        Ok("2") => crate::impl_2::run(),
        Ok("3") => crate::impl_3::run(),
        Ok("4") => crate::impl_4::run(),
        Ok("5") => crate::impl_5::run(),
        Ok("6") => crate::impl_6::run(),
        Ok(x) => bail!("{x}: Unknown impl"),
        Err(_) => crate::impl_5::run(),
    }
}

/// Respect `RUST_LOG`, falling back to INFO-level
fn log_init() {
    let filter = EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();
    let writer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    tracing_subscriber::registry()
        .with(filter)
        .with(writer)
        .init();
}
