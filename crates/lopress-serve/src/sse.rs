use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Default)]
pub struct Subscribers {
    inner: Arc<Mutex<Vec<TcpStream>>>,
}

impl Subscribers {
    pub fn add(&self, mut stream: TcpStream) -> std::io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\n\
             content-type: text/event-stream\r\n\
             cache-control: no-cache\r\n\
             connection: keep-alive\r\n\
             \r\n\
             retry: 1000\n\n"
        )?;
        stream.flush()?;
        self.inner.lock().unwrap().push(stream);
        Ok(())
    }

    pub fn broadcast_reload(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.retain_mut(|s| {
            write!(s, "event: reload\ndata: {{}}\n\n")
                .and_then(|_| s.flush())
                .is_ok()
        });
    }

    pub fn ping_loop(self) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut last = Instant::now();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if last.elapsed() < Duration::from_secs(15) {
                    continue;
                }
                last = Instant::now();
                let mut guard = self.inner.lock().unwrap();
                guard.retain_mut(|s| write!(s, ":ping\n\n").and_then(|_| s.flush()).is_ok());
            }
        })
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;

    #[test]
    fn add_writes_sse_headers_and_retry() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (server_side, _) = listener.accept().unwrap();
            let subs = Subscribers::default();
            subs.add(server_side).unwrap();
            subs
        });
        let mut client = TcpStream::connect(addr).unwrap();
        // Drain everything the server writes immediately.
        let mut buf = [0u8; 512];
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let n = client.read(&mut buf).unwrap();
        let s = std::str::from_utf8(&buf[..n]).unwrap();
        assert!(s.contains("text/event-stream"));
        assert!(s.contains("retry: 1000"));
        let subs = handle.join().unwrap();
        assert_eq!(subs.len(), 1);
    }

    #[test]
    fn broadcast_writes_reload_event() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server_thread = std::thread::spawn(move || {
            let (server_side, _) = listener.accept().unwrap();
            let subs = Subscribers::default();
            subs.add(server_side).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            subs.broadcast_reload();
            std::thread::sleep(Duration::from_millis(100));
        });
        let mut client = TcpStream::connect(addr).unwrap();
        let mut all = Vec::new();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut buf = [0u8; 512];
        // Read twice: once for handshake, once for broadcast.
        for _ in 0..2 {
            match client.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => all.extend_from_slice(&buf[..n]),
            }
        }
        let s = String::from_utf8_lossy(&all);
        assert!(s.contains("event: reload"), "got: {s}");
        server_thread.join().unwrap();
    }
}
