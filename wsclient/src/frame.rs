use bytes::{Buf, Bytes};

pub struct Frame<T = Bytes> {
    pub bytes: T,
    pub header_len: usize,
}

impl<T: AsRef<[u8]>> Frame<T> {
    pub fn header(&self) -> &[u8] {
        &self.bytes.as_ref()[..self.header_len]
    }

    pub fn payload(&self) -> &[u8] {
        &self.bytes.as_ref()[self.header_len..]
    }
}

// Defined here:
// https://www.iana.org/assignments/websocket/websocket.xhtml#opcode
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum OpCode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
    Custom(u8),
}

impl<T: AsRef<[u8]>> Frame<T> {
    pub fn raw_opcode(&self) -> u8 {
        self.bytes.as_ref()[0] & 0b0000_1111
    }

    pub fn opcode(&self) -> OpCode {
        match self.raw_opcode() {
            128.. => unreachable!(),
            0x0 => OpCode::Continuation,
            0x1 => OpCode::Text,
            0x2 => OpCode::Binary,
            0x8 => OpCode::Close,
            0x9 => OpCode::Ping,
            0xA => OpCode::Pong,
            x => OpCode::Custom(x),
        }
    }

    /// This should always be zero for spec-compliant frames
    pub fn reserved_bits(&self) -> u8 {
        self.bytes.as_ref()[0] & 0b0111_0000
    }

    pub fn fin(&self) -> bool {
        self.bytes.as_ref()[0] & 0b1000_0000 != 0
    }

    pub fn mask(&self) -> Option<[u8; 4]> {
        if self.bytes.as_ref()[1] & 0b1000_0000 == 0 {
            return None;
        }
        let mask = self.bytes.as_ref()[self.header_len - 4..self.header_len]
            .try_into()
            .unwrap();
        Some(mask)
    }

    pub fn unmasked_payload(&self) -> Vec<u8> {
        let mut payload = self.payload().to_vec();
        if let Some(mask) = self.mask() {
            for (i, v) in payload.iter_mut().enumerate() {
                *v ^= mask[i & 3];
            }
        }
        payload
    }
}

pub struct NeedMoreBytes(pub usize);

// Ensures that `buffer` contains at least `n` bytes
macro_rules! require_bytes {
    ($buffer:expr, $n:expr) => {{
        let n = $n;
        let rem = $buffer.remaining();
        if rem < n {
            return Err(NeedMoreBytes(n - rem));
        }
    }};
}

impl<'a> Frame<&'a [u8]> {
    pub fn from_slice(buffer: &'a [u8]) -> Result<Self, NeedMoreBytes> {
        let (header_len, payload_len) = parse_length(buffer)?;
        let total_len = header_len + payload_len;
        require_bytes!(buffer, total_len);
        Ok(Frame {
            bytes: &buffer[..total_len],
            header_len,
        })
    }
}

impl Frame {
    pub fn from_bytes(buffer: &mut impl Buf) -> Result<Self, NeedMoreBytes> {
        let (header_len, payload_len) = parse_length(buffer.chunk())?;
        let total_len = header_len + payload_len;
        require_bytes!(buffer, total_len);
        Ok(Frame {
            bytes: buffer.copy_to_bytes(total_len),
            header_len,
        })
    }
}

fn parse_length(buffer: &[u8]) -> Result<(usize, usize), NeedMoreBytes> {
    let mut header_len = 2;
    require_bytes!(buffer, header_len);

    let len_code = buffer[1] & 0b0111_1111;
    let payload_len = match len_code {
        ..126 => usize::from(len_code),
        126 => {
            header_len += 2;
            require_bytes!(buffer, header_len);
            usize::from(u16::from_be_bytes(buffer[2..4].try_into().unwrap()))
        }
        127 => {
            header_len += 8;
            require_bytes!(buffer, header_len);
            usize::try_from(u64::from_be_bytes(buffer[2..10].try_into().unwrap())).unwrap()
        }
        128.. => unreachable!(),
    };

    let is_masked = buffer[1] & 0b1000_0000 != 0;
    if is_masked {
        header_len += 4;
    }

    Ok((header_len, payload_len))
}
