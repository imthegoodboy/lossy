use interprocess::{
    local_socket::{GenericNamespaced, ListenerOptions, Stream, prelude::*},
    os::windows::{local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor},
};
use serde_json::Value;
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};

pub fn name() -> Result<String, String> {
    Ok(format!("lossy-{}", crate::platform::sid()?))
}
pub fn listener() -> Result<interprocess::local_socket::Listener, String> {
    let name = name()?;
    let sddl =
        widestring::U16CString::from_str(format!("D:P(A;;GA;;;{})", crate::platform::sid()?))
            .map_err(|_| "IPC identity unavailable")?;
    let sd = SecurityDescriptor::deserialize(&sddl).map_err(|_| "IPC permissions unavailable")?;
    ListenerOptions::new()
        .name(
            name.to_ns_name::<GenericNamespaced>()
                .map_err(|_| "IPC name unavailable")?,
        )
        .security_descriptor(sd)
        .create_sync()
        .map_err(|_| "Lossy is already running or its pipe is unavailable".into())
}
pub fn request(value: &Value) -> Result<Value, String> {
    let name = name()?;
    let mut stream = Stream::connect(
        name.to_ns_name::<GenericNamespaced>()
            .map_err(|_| "IPC name unavailable")?,
    )
    .map_err(|_| "Background process is not responding. Reopen Lossy to restart it.")?;
    stream
        .set_nonblocking(true)
        .map_err(|_| "IPC unavailable")?;
    send(&mut stream, value)?;
    let result = receive(&mut stream)?;
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        Err(error.into())
    } else {
        Ok(result["ok"].clone())
    }
}
const LIMIT: usize = 12 * 1024 * 1024;
fn transfer(stream: &mut Stream, bytes: &mut [u8], write: bool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut done = 0;
    while done < bytes.len() {
        let result = if write {
            stream.write(&bytes[done..])
        } else {
            stream.read(&mut bytes[done..])
        };
        match result {
            // Windows PIPE_NOWAIT may report a successful zero-byte read while the
            // peer is connected but has not written yet. It is not a Unix EOF.
            Ok(0) => {
                if Instant::now() > deadline {
                    return Err("Background process timed out".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(n) => done += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Err("Background process timed out".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(_) => return Err("Local communication failed".into()),
        }
    }
    Ok(())
}
pub fn receive(stream: &mut Stream) -> Result<Value, String> {
    let mut size = [0; 4];
    transfer(stream, &mut size, false)?;
    let size = u32::from_le_bytes(size) as usize;
    if size > LIMIT {
        return Err("Request too large".into());
    }
    let mut body = vec![0; size];
    transfer(stream, &mut body, false)?;
    serde_json::from_slice(&body).map_err(|_| "Invalid local request".into())
}
pub fn send(stream: &mut Stream, value: &Value) -> Result<(), String> {
    let mut body = serde_json::to_vec(value).map_err(|_| "Invalid response")?;
    if body.len() > LIMIT {
        return Err("Response too large. Use a smaller page.".into());
    }
    transfer(stream, &mut (body.len() as u32).to_le_bytes(), true)?;
    transfer(stream, &mut body, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_peer_can_delay_and_fragment_its_first_frame() {
        let name = format!("lossy-synthetic-ipc-{}", std::process::id());
        let listener = ListenerOptions::new()
            .name(name.clone().to_ns_name::<GenericNamespaced>().unwrap())
            .create_sync()
            .unwrap();
        let server = std::thread::spawn(move || {
            let mut stream = listener.accept().unwrap();
            stream.set_nonblocking(true).unwrap();
            let value = receive(&mut stream).unwrap();
            assert_eq!(value["test"], "synthetic");
            send(&mut stream, &value).unwrap();
        });
        let mut client = Stream::connect(name.to_ns_name::<GenericNamespaced>().unwrap()).unwrap();
        client.set_nonblocking(true).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        let bytes = br#"{"test":"synthetic"}"#;
        client
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .unwrap();
        std::thread::sleep(Duration::from_millis(30));
        client.write_all(bytes).unwrap();
        assert_eq!(receive(&mut client).unwrap()["test"], "synthetic");
        server.join().unwrap();
    }
}
