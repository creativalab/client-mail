//! One Ferrowl per user.
//!
//! The first process holds an exclusive lock on `instance.lock` and listens
//! on a loopback port written to `instance` (with a random token). A second
//! launch finds the lock taken, asks the first one to show its window and
//! exits, so clicking the icon again raises the window instead of opening a
//! new one.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

/// Requests from later launches, consumed by the app's subscription.
static RAISE: Mutex<Option<Receiver<()>>> = Mutex::new(None);

/// What `start` decided.
pub enum Role {
    /// This is the only instance (or the lock is unavailable): keep the
    /// guard alive for the whole run.
    Primary(#[allow(dead_code)] Option<File>),
    /// Another instance is running and has been asked to show itself.
    Secondary,
}

pub fn start() -> Role {
    let Some(dir) = dirs::data_local_dir().map(|d| d.join("ferrowl")) else {
        return Role::Primary(None);
    };
    let _ = fs::create_dir_all(&dir);
    let Ok(lock) = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("instance.lock"))
    else {
        // Better to run (possibly twice) than not at all.
        return Role::Primary(None);
    };

    if lock.try_lock().is_err() {
        // Someone holds the lock: wake it up. The first instance may still
        // be starting, so give it a moment to publish its port.
        for _ in 0..20 {
            if signal(&dir.join("instance")) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        return Role::Secondary;
    }

    listen(&dir.join("instance"));
    Role::Primary(Some(lock))
}

/// Takes the channel of raise requests (once).
pub fn take_raise_requests() -> Option<Receiver<()>> {
    RAISE.lock().ok()?.take()
}

fn listen(info: &std::path::Path) {
    let Ok(listener) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else {
        return;
    };
    let Ok(addr) = listener.local_addr() else {
        return;
    };
    let token = random_token();
    if write_private(info, &format!("{} {token}\n", addr.port())).is_err() {
        return;
    }
    let (tx, rx) = channel();
    if let Ok(mut slot) = RAISE.lock() {
        *slot = Some(rx);
    }
    std::thread::Builder::new()
        .name("single-instance".into())
        .spawn(move || serve(listener, token, tx))
        .ok();
}

fn serve(listener: TcpListener, token: String, tx: Sender<()>) {
    for stream in listener.incoming().flatten() {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut line = String::new();
        let mut reader = BufReader::new(&stream);
        if reader.read_line(&mut line).is_ok() && line.trim() == format!("{token} raise") {
            let _ = (&stream).write_all(b"ok\n");
            if tx.send(()).is_err() {
                return;
            }
        }
    }
}

/// Asks the running instance to show itself. True when it answered.
fn signal(info: &std::path::Path) -> bool {
    let Ok(text) = fs::read_to_string(info) else {
        return false;
    };
    let mut parts = text.split_whitespace();
    let (Some(port), Some(token)) = (parts.next().and_then(|p| p.parse::<u16>().ok()), parts.next()) else {
        return false;
    };
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    if stream.write_all(format!("{token} raise\n").as_bytes()).is_err() {
        return false;
    }
    let mut answer = String::new();
    BufReader::new(&stream).read_line(&mut answer).is_ok() && answer.trim() == "ok"
}

fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}

fn random_token() -> String {
    use std::hash::{BuildHasher, RandomState};
    let a = RandomState::new().hash_one(std::process::id());
    let b = RandomState::new().hash_one(std::time::SystemTime::now());
    format!("{a:016x}{b:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_launch_reaches_the_first() {
        let dir = std::env::temp_dir().join(format!("ferrowl-si-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let info = dir.join("instance");
        listen(&info);
        let rx = take_raise_requests().expect("listener started");
        assert!(signal(&info));
        rx.recv_timeout(Duration::from_secs(2))
            .expect("raise request delivered");

        // Wrong token: refused.
        let port = fs::read_to_string(&info)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned();
        fs::write(&info, format!("{port} nope\n")).unwrap();
        assert!(!signal(&info));
        fs::remove_dir_all(&dir).unwrap();
    }
}
