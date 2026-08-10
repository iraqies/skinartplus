// Minimal Minecraft Java protocol codec: varint, strings, packet framing,
// zlib compression and AES/CFB8 encryption.

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes128;
use flate2::read::ZlibDecoder;
use std::io::{self, Read, Write as StdWrite};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub const MAX_PACKET_SIZE: usize = 2 * 1024 * 1024;

/// AES/CFB8 stream cipher as used by the Minecraft protocol (same key in both directions).
pub struct AesCfb8 {
    cipher: Aes128,
    reg: [u8; 16],
    decrypting: bool,
}

impl AesCfb8 {
    pub fn new(key: &[u8; 16]) -> Self {
        let key_arr = GenericArray::clone_from_slice(key);
        AesCfb8 {
            cipher: Aes128::new(&key_arr),
            reg: *key,
            decrypting: false,
        }
    }

    pub fn new_decrypt(key: &[u8; 16]) -> Self {
        let mut c = Self::new(key);
        c.decrypting = true;
        c
    }

    pub fn process(&mut self, data: &mut [u8]) {
        for i in 0..data.len() {
            let mut block = GenericArray::clone_from_slice(&self.reg);
            self.cipher.encrypt_block(&mut block);
            // CFB8 shifts the register by the *ciphertext* byte: that is the
            // output byte when encrypting, but the input byte when decrypting.
            let raw = data[i];
            data[i] = raw ^ block[0];
            self.reg.copy_within(1.., 0);
            self.reg[15] = if self.decrypting { raw } else { data[i] };
        }
    }
}

pub struct McStream {
    stream: TcpStream,
    encrypt: Option<AesCfb8>,
    decrypt: Option<AesCfb8>,
    compression_threshold: i32,
}

pub fn varint_bytes(v: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut val = v as u32;
    loop {
        let mut b = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            b |= 0x80;
        }
        out.push(b);
        if val == 0 {
            break;
        }
    }
    out
}

/// Parse a varint from a byte slice, returning (value, bytes consumed).
pub fn parse_varint(data: &[u8]) -> Result<(i32, usize), String> {
    let mut val: u32 = 0;
    let mut shift = 0u32;
    let mut i = 0;
    while i < data.len() && i < 5 {
        let b = data[i];
        val |= ((b & 0x7F) as u32) << shift;
        i += 1;
        if b & 0x80 == 0 {
            return Ok((val as i32, i));
        }
        shift += 7;
    }
    Err("invalid varint".into())
}

/// Parse a length-prefixed UTF-8 string from a byte slice.
pub fn parse_string(data: &[u8]) -> Result<(String, usize), String> {
    let (len, i) = parse_varint(data)?;
    if len < 0 || (i + len as usize) > data.len() {
        return Err("invalid string".into());
    }
    let s = std::str::from_utf8(&data[i..i + len as usize])
        .map_err(|e| e.to_string())?
        .to_string();
    Ok((s, i + len as usize))
}

impl McStream {
    pub fn new(stream: TcpStream) -> Self {
        McStream {
            stream,
            encrypt: None,
            decrypt: None,
            compression_threshold: -1,
        }
    }

    pub fn enable_encryption(&mut self, key: &[u8; 16]) {
        self.encrypt = Some(AesCfb8::new(key));
        self.decrypt = Some(AesCfb8::new_decrypt(key));
    }

    pub fn set_compression(&mut self, threshold: i32) {
        self.compression_threshold = threshold;
    }

    async fn read_byte(&mut self) -> io::Result<u8> {
        let mut b = [0u8; 1];
        self.stream.read_exact(&mut b).await?;
        if let Some(d) = &mut self.decrypt {
            d.process(&mut b);
        }
        Ok(b[0])
    }

    async fn read_raw(&mut self, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;
        if let Some(d) = &mut self.decrypt {
            d.process(&mut buf);
        }
        Ok(buf)
    }

    pub async fn read_varint(&mut self) -> io::Result<i32> {
        let mut val: u32 = 0;
        let mut shift = 0u32;
        loop {
            let b = self.read_byte().await?;
            val |= ((b & 0x7F) as u32) << shift;
            if b & 0x80 == 0 {
                return Ok(val as i32);
            }
            shift += 7;
            if shift >= 35 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "varint too long"));
            }
        }
    }

    pub async fn read_string(&mut self) -> io::Result<String> {
        let len = self.read_varint().await?;
        if len < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "negative string len"));
        }
        let buf = self.read_raw(len as usize).await?;
        String::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub async fn read_packet(&mut self) -> Result<(i32, Vec<u8>), String> {
        let len = self.read_varint().await.map_err(|e| e.to_string())?;
        if len < 0 || len as usize > MAX_PACKET_SIZE {
            return Err(format!("oversized packet: {}", len));
        }
        let mut data = self.read_raw(len as usize).await.map_err(|e| e.to_string())?;
        if self.compression_threshold >= 0 {
            if data.is_empty() {
                return Err("empty compressed packet".into());
            }
            let (uncompressed_len, consumed) = parse_varint(&data)?;
            if uncompressed_len == 0 {
                data = data[consumed..].to_vec();
            } else {
                let mut dec = ZlibDecoder::new(&data[consumed..]);
                let mut out = Vec::with_capacity(uncompressed_len as usize);
                dec.read_to_end(&mut out).map_err(|e| e.to_string())?;
                data = out;
            }
        }
        let (id, consumed) = parse_varint(&data)?;
        let payload = data[consumed..].to_vec();
        Ok((id, payload))
    }

    fn encrypt_buf(&mut self, buf: &mut [u8]) {
        if let Some(e) = &mut self.encrypt {
            e.process(buf);
        }
    }

    pub async fn write_varint(&mut self, v: i32) -> io::Result<()> {
        let bytes = varint_bytes(v);
        self.write_raw(&bytes).await
    }

    async fn write_raw(&mut self, buf: &[u8]) -> io::Result<()> {
        let mut b = buf.to_vec();
        self.encrypt_buf(&mut b);
        self.stream.write_all(&b).await
    }

    pub async fn write_string(&mut self, s: &str) -> io::Result<()> {
        self.write_varint(s.len() as i32).await?;
        self.write_raw(s.as_bytes()).await
    }

    pub async fn write_packet(&mut self, id: i32, payload: &[u8]) -> Result<(), String> {
        let mut body = Vec::with_capacity(payload.len() + 5);
        body.extend(varint_bytes(id));
        body.extend_from_slice(payload);

        let frame: Vec<u8>;
        if self.compression_threshold >= 0 {
            if body.len() >= self.compression_threshold as usize {
                let mut enc =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                StdWrite::write_all(&mut enc, &body).map_err(|e| e.to_string())?;
                let comp = enc.finish().map_err(|e| e.to_string())?;
                let mut inner = Vec::with_capacity(comp.len() + 5);
                inner.extend(varint_bytes(body.len() as i32));
                inner.extend(&comp);
                let mut f = Vec::with_capacity(inner.len() + 5);
                f.extend(varint_bytes(inner.len() as i32));
                f.extend(inner);
                frame = f;
            } else {
                let mut inner = Vec::with_capacity(body.len() + 5);
                inner.extend(varint_bytes(0));
                inner.extend(&body);
                let mut f = Vec::with_capacity(inner.len() + 5);
                f.extend(varint_bytes(inner.len() as i32));
                f.extend(inner);
                frame = f;
            }
        } else {
            let mut f = Vec::with_capacity(body.len() + 5);
            f.extend(varint_bytes(body.len() as i32));
            f.extend(&body);
            frame = f;
        }
        self.write_raw(&frame).await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod cfb8_reference {
    use super::*;

    #[test]
    fn matches_python_cfb8() {
        let key: [u8; 16] = (0u8..16).collect::<Vec<u8>>().try_into().unwrap();
        let pt = b"Hello, Minecraft CFB8 test!";
        let expect_ct = "42ea5ed4daf864eae7ef5c17728310d5ec9c8966a7882537258862";

        let mut enc = AesCfb8::new(&key);
        let mut buf = pt.to_vec();
        enc.process(&mut buf);
        println!("ct  = {}", crate::mc::crypto::hex(&buf));
        assert_eq!(crate::mc::crypto::hex(&buf), expect_ct);

        let mut dec = AesCfb8::new_decrypt(&key);
        dec.process(&mut buf);
        println!("pt  = {}", crate::mc::crypto::hex(&buf));
        assert_eq!(buf, pt);
    }
}
