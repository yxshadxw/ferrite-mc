use crate::cfb8::Cfb8;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

pub struct Wire {
    stream: TcpStream,
    read: Option<Cfb8>,
    write: Option<Cfb8>,
    /// Ciphertext for the packet currently being written. `poll_write` only
    /// hands us an immutable slice, so the buffer is encrypted in place here
    /// and handed to the socket in one call. Reused between writes.
    scratch: Vec<u8>,
}
impl Wire {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            read: None,
            write: None,
            scratch: Vec::new(),
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
                    cipher.decrypt(&mut buf.filled_mut()[before..]);
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
        let this = self.as_mut().get_mut();
        if this.write.is_none() {
            return Pin::new(&mut this.stream).poll_write(cx, data);
        }
        let cipher = this.write.as_mut().unwrap();
        // CFB8 has no rewind, so keep the state around until we know how many
        // bytes the socket took: a short write has to be replayed from here.
        let saved = cipher.state();
        let mut scratch = std::mem::take(&mut this.scratch);
        scratch.clear();
        scratch.extend_from_slice(data);
        cipher.encrypt(&mut scratch);
        let result = Pin::new(&mut this.stream).poll_write(cx, &scratch);
        match result {
            // Common case: the whole packet went out after a single pass over
            // the data (the old path encrypted it twice).
            Poll::Ready(Ok(written)) if written == data.len() => {
                this.scratch = scratch;
                Poll::Ready(Ok(written))
            }
            Poll::Ready(Ok(written)) => {
                // Short write: rewind and advance by exactly the bytes the
                // socket accepted, so the caller retries with `data[written..]`.
                cipher.restore(saved);
                let mut accepted = data[..written].to_vec();
                cipher.encrypt(&mut accepted);
                this.scratch = scratch;
                Poll::Ready(Ok(written))
            }
            Poll::Ready(Err(error)) => {
                cipher.restore(saved);
                this.scratch = scratch;
                Poll::Ready(Err(error))
            }
            Poll::Pending => {
                // Nothing was written, so nothing may be consumed.
                cipher.restore(saved);
                this.scratch = scratch;
                Poll::Pending
            }
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
    use crate::cfb8::Cfb8;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const KEY: [u8; 16] = *b"ferrite-test-key";

    /// A 3 MiB payload forces the socket buffers full, so the writer goes
    /// through `Pending` and short writes instead of the happy path only.
    #[tokio::test]
    async fn encrypted_stream_roundtrips_under_backpressure() {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let plaintext: Vec<u8> = (0..3 * 1024 * 1024).map(|i| (i * 7 + 3) as u8).collect();
            let expected = plaintext.clone();

            let writer = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut wire = Wire::new(stream);
                wire.enable_encryption(&KEY);
                for chunk in plaintext.chunks(65_521) {
                    wire.write_all(chunk).await.unwrap();
                }
                wire.flush().await.unwrap();
            });

            let mut reader = tokio::net::TcpStream::connect(address).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let mut cipher = Cfb8::new(&KEY);
            let mut received = Vec::with_capacity(expected.len());
            let mut buffer = vec![0u8; 8192];
            while received.len() < expected.len() {
                let read = reader.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0, "connection closed early");
                cipher.decrypt(&mut buffer[..read]);
                received.extend_from_slice(&buffer[..read]);
            }
            writer.await.unwrap();
            assert_eq!(received, expected);
        })
        .await
        .expect("encrypted transfer timed out");
    }
}
