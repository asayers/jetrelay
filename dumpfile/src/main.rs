use anyhow::Context;
use bpaf::{Bpaf, Parser};
use jiff::Timestamp;
use std::path::PathBuf;
use wsclient::OpCode;

#[derive(Bpaf)]
struct Opts {
    #[bpaf(positional("PATH"))]
    path: PathBuf,
}
fn main() -> anyhow::Result<()> {
    let opts = opts().to_options().run();
    for frame in wsclient::read_websocket(&opts.path)? {
        let frame = frame?;
        assert_eq!(frame.opcode(), OpCode::Text);
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
            })?
            .parse()?;
        let collection = gjson::get(&payload, "commit.collection");
        let text = gjson::get(&payload, "commit.record.text");
        let timestamp = Timestamp::from_microsecond(timestamp as i64).unwrap();
        println!(
            "[{timestamp}] {collection} ({} bytes) {text}",
            payload.len()
        );
    }
    Ok(())
}
