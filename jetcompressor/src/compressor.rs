use fsst::{CompressorBuilder, Symbol};
use std::fmt;

/// A compressor which actually uses the symbol table you give it!
pub struct Compressor {
    inner: fsst::Compressor,
    map_to: [u8; 256],
    map_from: [u8; 256],
}

impl Compressor {
    pub fn new(symbols: &[&str]) -> Self {
        assert!(symbols.len() < 256);
        for s in symbols {
            assert!(s.len() <= 8);
        }

        let mut bldr = CompressorBuilder::new();
        for sym in symbols {
            let mut bytes = [0; 8];
            let n = sym.len();
            bytes[..n].copy_from_slice(sym.as_bytes());
            let sym2 = Symbol::from_slice(&bytes);
            let ok = bldr.insert(sym2, n);
            if !ok {
                panic!("Couldn't insert symbol: {sym}");
            }
        }
        let cmprsr = bldr.build();
        // let mut seed = 0;
        // let cmprsr = 'outer: loop {
        //     eprintln!("Trying seed {seed}...");
        //     let mut bldr = CompressorBuilder::with_seed(seed);
        //     for sym in symbols {
        //         let mut bytes = [0; 8];
        //         let n = sym.len();
        //         bytes[..n].copy_from_slice(sym.as_bytes());
        //         let sym2 = Symbol::from_slice(&bytes);
        //         let ok = bldr.insert(sym2, n);
        //         if !ok {
        //             eprintln!("Couldn't insert symbol: {sym}");
        //             seed += 1;
        //             continue 'outer;
        //         }
        //     }
        //     break bldr.build();
        // };

        let mut map_to = [0xff_u8; 256];
        let mut map_from = [0xff_u8; 256];

        for (i, sym) in cmprsr.symbol_table().iter().enumerate() {
            let j = symbols
                .iter()
                .position(|s| symbol_eq(str_to_symbol(s), *sym))
                .unwrap();
            map_to[i] = j as u8;
            map_from[j] = i as u8;
        }
        // dbg!(map_to);

        let n = cmprsr.symbol_table().len();
        assert_eq!(n, symbols.len());

        if let Err(x) = is_permutation(&map_to[..n]) {
            panic!("{x} ({})", symbols[x as usize]);
        }
        is_permutation(&map_from[..n]).unwrap();
        for x in 0..n {
            assert_eq!(map_from[map_to[x] as usize], x as u8);
            assert_eq!(map_to[map_from[x] as usize], x as u8);
        }
        // Check that the magic code maps to itself
        assert_eq!(map_to[0xff], 0xff);

        Compressor {
            inner: cmprsr,
            map_from,
            map_to,
        }
    }

    pub fn train(values: &Vec<&[u8]>) -> Self {
        let mut map = [0; 256];
        for (i, x) in map.iter_mut().enumerate() {
            *x = i as u8;
        }
        Compressor {
            inner: fsst::Compressor::train(values),
            map_to: map,
            map_from: map,
        }
    }

    pub fn compress(&self, xs: &[u8]) -> Vec<u8> {
        let mut compressed = self.inner.compress(xs);
        for x in &mut compressed {
            *x = self.map_to[*x as usize];
        }
        compressed
    }
}

impl fmt::Display for Compressor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in self.map_from {
            if let Some(sym) = self.inner.symbol_table().get(i as usize) {
                write!(f, "{sym:?} ")?;
            }
        }
        Ok(())
    }
}

fn is_permutation(xs: &[u8]) -> Result<(), u8> {
    let mut xs = xs.to_vec();
    xs.sort();
    for (x, y) in xs.into_iter().zip(0..) {
        if x != y {
            return Err(x);
        }
    }
    Ok(())
}

fn str_to_symbol(s: &str) -> Symbol {
    let mut bytes = [0; 8];
    let n = s.len();
    bytes[..n].copy_from_slice(s.as_bytes());
    Symbol::from_slice(&bytes)
}

// Very silly
fn symbol_eq(x: Symbol, y: Symbol) -> bool {
    format!("{x:?}") == format!("{y:?}")
}
