use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;

pub struct Request {
    pub method: String,
    pub path: String,
    #[allow(dead_code)]
    pub headers: Vec<(String, String)>,
}

pub fn read_request(stream: &TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    if method.is_empty() || path.is_empty() {
        return Ok(None);
    }

    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h)?;
        if n == 0 {
            break;
        }
        let trimmed = h.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    Ok(Some(Request {
        method,
        path,
        headers,
    }))
}

pub fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<()> {
    use std::io::Write;
    write!(stream, "HTTP/1.1 {status} {reason}\r\n")?;
    let mut has_len = false;
    let mut has_type = false;
    for (k, v) in headers {
        write!(stream, "{k}: {v}\r\n")?;
        if k.eq_ignore_ascii_case("content-length") {
            has_len = true;
        }
        if k.eq_ignore_ascii_case("content-type") {
            has_type = true;
        }
    }
    if !has_len {
        write!(stream, "content-length: {}\r\n", body.len())?;
    }
    if !has_type {
        write!(stream, "content-type: application/octet-stream\r\n")?;
    }
    write!(stream, "connection: close\r\n\r\n")?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}

/// Read and discard the request body (if any) so we can close cleanly.
#[allow(dead_code)]
pub fn drain(stream: &TcpStream) {
    let mut buf = [0u8; 1024];
    let mut s = stream.try_clone().unwrap();
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(50)));
    while s.read(&mut buf).unwrap_or(0) > 0 {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn roundtrip_request(raw: &[u8]) -> Option<(String, String)> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            read_request(&stream).unwrap()
        });
        let mut client = TcpStream::connect(addr).unwrap();
        client.write_all(raw).unwrap();
        drop(client);
        let req = handle.join().unwrap()?;
        Some((req.method, req.path))
    }

    #[test]
    fn parses_simple_get() {
        let (m, p) = roundtrip_request(b"GET /foo HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();
        assert_eq!(m, "GET");
        assert_eq!(p, "/foo");
    }

    #[test]
    fn empty_connection_returns_none() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            read_request(&stream).unwrap()
        });
        drop(TcpStream::connect(addr).unwrap());
        assert!(handle.join().unwrap().is_none());
    }
}
