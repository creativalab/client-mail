//! Minimal POP3 client (RFC 1939, STLS from RFC 2595): enough to download
//! new messages and delete old ones from the server.
//!
//! Port 995 uses TLS from the start; any other port connects in plain text
//! and must upgrade with STLS (the password is never sent unencrypted).

use crate::config::Account;
use crate::i18n::tf;
use anyhow::{Context, Result, anyhow, bail};
use rustls::pki_types::ServerName;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(30);

trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

pub struct Pop3 {
    io: BufReader<Box<dyn Stream>>,
}

/// Number and unique id of a message on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub number: u32,
    pub uidl: String,
}

fn tls(host: &str, tcp: TcpStream) -> Result<Box<dyn Stream>> {
    let name = ServerName::try_from(host.to_owned()).context("server name")?;
    let conn = rustls::ClientConnection::new(crate::mail::tls_config(), name)?;
    Ok(Box::new(rustls::StreamOwned::new(conn, tcp)))
}

impl Pop3 {
    /// Connects and logs in with the account's incoming server settings.
    pub fn connect(account: &Account) -> Result<Pop3> {
        let host = account.imap_host.trim();
        let port = account.imap_port;
        let addr = format!("{host}:{port}");
        let cannot = || tf("err_connect", &[("host", host), ("port", &port.to_string())]);
        let tcp = std::net::ToSocketAddrs::to_socket_addrs(&addr)
            .with_context(cannot)?
            .find_map(|a| TcpStream::connect_timeout(&a, TIMEOUT).ok())
            .ok_or_else(|| anyhow!(cannot()))?;
        tcp.set_read_timeout(Some(TIMEOUT))?;
        tcp.set_write_timeout(Some(TIMEOUT))?;

        let mut pop = if port == 995 {
            let mut p = Pop3 {
                io: BufReader::new(tls(host, tcp)?),
            };
            p.ok_line()?;
            p
        } else {
            let mut plain = Pop3 {
                io: BufReader::new(Box::new(tcp.try_clone()?)),
            };
            plain.ok_line()?;
            plain
                .command("STLS")
                .map_err(|_| anyhow!(tf("err_pop3_stls", &[])))?;
            Pop3 {
                io: BufReader::new(tls(host, tcp)?),
            }
        };
        pop.command(&format!("USER {}", account.username.trim()))?;
        pop.command(&format!("PASS {}", account.password))
            .map_err(|e| anyhow!(tf("err_pop3_login", &[("msg", &e.to_string())])))?;
        Ok(pop)
    }

    fn read_line(&mut self) -> Result<String> {
        let mut line = String::new();
        if self.io.read_line(&mut line)? == 0 {
            bail!("connection closed by the server");
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    }

    /// Reads a status line: `+OK ...` or an error with the server's text.
    fn ok_line(&mut self) -> Result<String> {
        let line = self.read_line()?;
        match line.strip_prefix("+OK") {
            Some(rest) => Ok(rest.trim().to_owned()),
            None => bail!("{}", line.strip_prefix("-ERR").unwrap_or(&line).trim()),
        }
    }

    fn command(&mut self, cmd: &str) -> Result<String> {
        let stream = self.io.get_mut();
        stream.write_all(cmd.as_bytes())?;
        stream.write_all(b"\r\n")?;
        stream.flush()?;
        self.ok_line()
    }

    /// Multi-line answer body, dot-unstuffed, lines joined with CRLF.
    fn multiline(&mut self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let mut line = Vec::new();
            if self.io.read_until(b'\n', &mut line)? == 0 {
                bail!("connection closed by the server");
            }
            let content = line.strip_suffix(b"\n").unwrap_or(&line);
            let content = content.strip_suffix(b"\r").unwrap_or(content);
            if content == b"." {
                return Ok(out);
            }
            let content = content.strip_prefix(b".").unwrap_or(content);
            out.extend_from_slice(content);
            out.extend_from_slice(b"\r\n");
        }
    }

    /// Unique ids of every message on the server.
    pub fn uidl(&mut self) -> Result<Vec<Listing>> {
        self.command("UIDL")?;
        Ok(parse_uidl(&self.multiline()?))
    }

    /// Full raw message.
    pub fn retr(&mut self, number: u32) -> Result<Vec<u8>> {
        self.command(&format!("RETR {number}"))?;
        self.multiline()
    }

    /// Marks a message for deletion (applied at `quit`).
    pub fn dele(&mut self, number: u32) -> Result<()> {
        self.command(&format!("DELE {number}")).map(|_| ())
    }

    /// Ends the session; the server applies the deletions now.
    pub fn quit(mut self) -> Result<()> {
        self.command("QUIT").map(|_| ())
    }
}

fn parse_uidl(body: &[u8]) -> Vec<Listing> {
    String::from_utf8_lossy(body)
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let number = parts.next()?.parse().ok()?;
            let uidl = parts.next()?.to_owned();
            Some(Listing { number, uidl })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(server: &'static [u8]) -> Pop3 {
        // Reads come from `server`; writes are discarded.
        struct Fake(std::io::Cursor<&'static [u8]>);
        impl Read for Fake {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                self.0.read(buf)
            }
        }
        impl Write for Fake {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        Pop3 {
            io: BufReader::new(Box::new(Fake(std::io::Cursor::new(server)))),
        }
    }

    #[test]
    fn lists_and_retrieves() {
        let mut p = fake(
            b"+OK 2 messages\r\n1 abc\r\n2 def\r\n.\r\n\
              +OK message follows\r\nSubject: hi\r\n\r\n..dot line\r\nend\r\n.\r\n\
              -ERR no such message\r\n",
        );
        assert_eq!(
            p.uidl().unwrap(),
            vec![
                Listing {
                    number: 1,
                    uidl: "abc".into()
                },
                Listing {
                    number: 2,
                    uidl: "def".into()
                }
            ]
        );
        assert_eq!(p.retr(1).unwrap(), b"Subject: hi\r\n\r\n.dot line\r\nend\r\n");
        assert_eq!(p.dele(9).unwrap_err().to_string(), "no such message");
    }
}

#[cfg(test)]
mod live {
    /// Read-only check against the configured account's POP3 server:
    /// `FERROWL_POP3_HOST=pop3.example.com cargo test live_pop3 -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_pop3() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut acc = crate::config::Config::load()
            .account()
            .cloned()
            .expect("configured account");
        acc.imap_host = std::env::var("FERROWL_POP3_HOST").expect("FERROWL_POP3_HOST");
        acc.imap_port = std::env::var("FERROWL_POP3_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(995);
        let mut p = super::Pop3::connect(&acc).expect("login");
        let list = p.uidl().expect("UIDL");
        println!("messages on server: {}", list.len());
        if let Some(last) = list.last() {
            let raw = p.retr(last.number).expect("RETR");
            let h = crate::mail::header_from_raw(1, &raw, false);
            println!(
                "newest: {} bytes, has subject: {}",
                raw.len(),
                !h.subject.is_empty()
            );
        }
        p.quit().expect("QUIT");
    }
}
