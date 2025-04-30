mod compressor;
mod table;

use crate::compressor::Compressor;
use crate::table::PRESET_SYMBOLS;
use bpaf::{Bpaf, Parser};
use bstr::ByteSlice;
use url::Url;
use wsclient::Frame;

#[derive(Bpaf)]
struct Opts {
    train: bool,
    print: bool,
    #[bpaf(positional("URL"))]
    url: Url,
}

fn main() {
    let opts = opts().to_options().run();
    let url = format!("{}?cursor=0", opts.url).parse().unwrap();
    let iter = wsclient::connect_websocket(&url).unwrap();
    let frames: Vec<Frame> = iter
        .map(|f| {
            let frame = f.unwrap();
            assert_eq!(frame.reserved_bits(), 0);
            assert_eq!(frame.opcode(), wsclient::OpCode::Text);
            assert!(frame.mask().is_none());
            frame
        })
        .take(20_000)
        .collect();
    let payloads: Vec<&[u8]> = frames.iter().map(|frame| frame.payload()).collect();
    let cmprsr = if opts.train {
        println!("Training on {} frames...", payloads.len());
        let x = Compressor::train(&payloads);
        println!();
        println!("Freshly-trained:");
        x
    } else {
        println!("Preset:");
        Compressor::new(&PRESET_SYMBOLS)
    };
    if opts.print {
        for payload in &payloads {
            let compressed = cmprsr.compress(payload);
            println!("{} <<< {}", compressed.escape_ascii(), payload.as_bstr());
            // println!("{}", compressed.as_bstr());
            // println!("{}", payload.as_bstr());
            // ratio(payload.len(), compressed.len());
        }
    }
    println!("{cmprsr}");
    evaluate(&payloads, &cmprsr);
}

fn evaluate(payloads: &[&[u8]], cmprsr: &Compressor) {
    let mut n_uncompressed = 0;
    let mut n_compressed = 0;
    for &payload in payloads {
        let compressed = cmprsr.compress(payload);
        n_uncompressed += payload.len();
        n_compressed += compressed.len();
        // println!("{} <<< {}", compressed.escape_ascii(), payload.as_bstr());
        // println!("{}", compressed.as_bstr());
        // println!("{}", payload.as_bstr());
        // ratio(payload.len(), compressed.len());
    }
    ratio(n_uncompressed, n_compressed);
}

fn ratio(before: usize, after: usize) {
    println!(
        "{before} => {after} ({:.1}%)",
        after as f64 / before as f64 * 100.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let cmprsr = Compressor::new(&PRESET_SYMBOLS);
        let x = cmprsr.compress(b"abc");
        assert_eq!(x, b"abc");
    }
}
