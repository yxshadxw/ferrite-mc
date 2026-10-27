use aes::{
    Aes128,
    cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray},
};
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
#[derive(Clone)]
struct Cfb8 {
    cipher: Aes128,
    shift: [u8; 16],
}
impl Cfb8 {
    fn new(key: &[u8; 16]) -> Self {
        Self::with_iv(key, key)
    }
    fn with_iv(key: &[u8; 16], iv: &[u8; 16]) -> Self {
        Self {
            cipher: Aes128::new(GenericArray::from_slice(key)),
            shift: *iv,
        }
    }
    fn apply(&mut self, data: &mut [u8], decrypt: bool) {
        for byte in data {
            let input = *byte;
            let mut block = GenericArray::clone_from_slice(&self.shift);
            self.cipher.encrypt_block(&mut block);
            *byte ^= block[0];
            self.shift.copy_within(1.., 0);
            self.shift[15] = if decrypt { input } else { *byte };
        }
    }
}
pub struct Wire {
    stream: TcpStream,
    read: Option<Cfb8>,
    write: Option<Cfb8>,
}
impl Wire {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            read: None,
            write: None,
        }
    }
    pub fn enable_encryption(&mut self, key: &[u8; 16]) {
        self.read = Some(Cfb8::new(key));
        self.write = Some(Cfb8::new(key));
    }
}
impl AsyncRead for Wire {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        match Pin::new(&mut self.stream).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                if let Some(cipher) = &mut self.read {
                    cipher.apply(&mut buf.filled_mut()[before..], true);
                }
                Poll::Ready(Ok(()))
            }
            v => v,
        }
    }
}
impl AsyncWrite for Wire {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if let Some(cipher) = &self.write {
            let mut next = cipher.clone();
            let mut encrypted = data.to_vec();
            next.apply(&mut encrypted, false);
            match Pin::new(&mut self.stream).poll_write(cx, &encrypted) {
                Poll::Ready(Ok(n)) => {
                    let mut consumed = data[..n].to_vec();
                    self.write.as_mut().unwrap().apply(&mut consumed, false);
                    Poll::Ready(Ok(n))
                }
                v => v,
            }
        } else {
            Pin::new(&mut self.stream).poll_write(cx, data)
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nist_cfb8_vector() {
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let iv = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
        let mut cipher = Cfb8::with_iv(&key, &iv);
        let mut data = [0x6b, 0xc1, 0xbe, 0xe2];
        cipher.apply(&mut data, false);
        assert_eq!(data, [0x3b, 0x79, 0x42, 0x4c]);
    }
}
