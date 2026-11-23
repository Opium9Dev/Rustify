use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::Duration;

const APP_ID: &str = "1573518152266940556";

pub struct Presence {
    pub details: String,
    pub state: String,
    pub image: String,
    pub image_text: String,
    pub url: String,
    pub start: i64,
    pub end: Option<i64>,
}

pub enum Msg {
    Set(Presence),
    Clear,
}

trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

#[cfg(windows)]
fn open() -> Option<Box<dyn Pipe>> {
    (0..10)
        .find_map(|i| {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(format!(r"\\.\pipe\discord-ipc-{i}"))
                .ok()
        })
        .map(|f| Box::new(f) as Box<dyn Pipe>)
}

#[cfg(unix)]
fn open() -> Option<Box<dyn Pipe>> {
    let dir = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .find_map(|k| std::env::var(k).ok())
        .unwrap_or_else(|| "/tmp".into());
    (0..10)
        .find_map(|i| std::os::unix::net::UnixStream::connect(format!("{dir}/discord-ipc-{i}")).ok())
        .map(|s| Box::new(s) as Box<dyn Pipe>)
}

#[cfg(not(any(windows, unix)))]
fn open() -> Option<Box<dyn Pipe>> {
    None
}

fn write_frame(c: &mut dyn Pipe, op: u32, v: &Value) -> std::io::Result<()> {
    let body = v.to_string().into_bytes();
    let mut buf = Vec::with_capacity(8 + body.len());
    buf.extend_from_slice(&op.to_le_bytes());
    buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
    buf.extend_from_slice(&body);
    c.write_all(&buf)?;
    c.flush()
}

fn read_frame(c: &mut dyn Pipe) -> std::io::Result<(u32, Vec<u8>)> {
    let mut h = [0u8; 8];
    c.read_exact(&mut h)?;
    let op = u32::from_le_bytes([h[0], h[1], h[2], h[3]]);
    let len = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
    if len > 1 << 20 {
        return Err(std::io::Error::other("Discord frame is too large"));
    }
    let mut body = vec![0u8; len];
    c.read_exact(&mut body)?;
    Ok((op, body))
}

fn log(msg: &str) {
    let dir = crate::app::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("discord.log"))
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn connect() -> Result<Box<dyn Pipe>, String> {
    let mut c = open().ok_or("Discord IPC pipe discord-ipc-0..9 not found; is the Discord desktop app running?")?;
    write_frame(c.as_mut(), 0, &json!({ "v": 1, "client_id": APP_ID }))
        .map_err(|e| format!("handshake write failed: {e}"))?;
    let (op, body) = read_frame(c.as_mut()).map_err(|e| format!("handshake read failed: {e}"))?;
    let txt = String::from_utf8_lossy(&body).chars().take(200).collect::<String>();
    if op == 2 {
        return Err(format!("Discord rejected the connection: {txt}"));
    }
    log(&format!("Connected to Discord (opcode {op}): {txt}"));
    Ok(c)
}

fn clip(s: &str) -> String {
    let mut t: String = s.chars().take(120).collect();
    while t.chars().count() < 2 {
        t.push('\u{200B}');
    }
    t
}

fn send(c: &mut dyn Pipe, nonce: &mut u64, p: Option<&Presence>, lvl: u8) -> std::io::Result<Result<(), String>> {
    *nonce += 1;
    let activity = p.map(|p| {
        let mut ts = json!({ "start": p.start });
        if let Some(e) = p.end {
            ts["end"] = json!(e);
        }
        let mut a = json!({
            "type": if lvl >= 3 { 0 } else { 2 },
            "details": clip(&p.details),
            "state": clip(&p.state),
            "timestamps": ts,
        });
        if lvl < 2 && !p.image.is_empty() && p.image.len() < 256 {
            a["assets"] = json!({ "large_image": p.image, "large_text": clip(&p.image_text) });
        }
        if lvl < 1 && !p.url.is_empty() {
            a["buttons"] = json!([{ "label": "Listen on YouTube Music", "url": p.url }]);
        }
        a
    });
    let msg = json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": std::process::id(), "activity": activity },
        "nonce": nonce.to_string(),
    });
    write_frame(c, 1, &msg)?;
    let (_, body) = read_frame(c)?;
    let reply: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if reply["evt"] == "ERROR" {
        let m = reply["data"]["message"].as_str().unwrap_or("unknown error");
        let code = reply["data"]["code"].as_i64().unwrap_or(0);
        return Ok(Err(format!("{m} (code {code})")));
    }
    Ok(Ok(()))
}

fn deliver(c: &mut dyn Pipe, nonce: &mut u64, p: Option<&Presence>) -> bool {
    let mut lvl = 0u8;
    loop {
        match send(c, nonce, p, lvl) {
            Ok(Ok(())) => {
                if let Some(p) = p {
                    log(&format!("Presence sent (level {lvl}): {} — {}", p.details, p.state));
                } else {
                    log("Presence cleared");
                }
                return true;
            }
            Ok(Err(e)) => {
                log(&format!("Discord rejected the activity (level {lvl}): {e}"));
                if p.is_none() || lvl >= 3 {
                    return true;
                }
                lvl += 1;
            }
            Err(e) => {
                log(&format!("IPC pipe error: {e}"));
                return false;
            }
        }
    }
}

pub fn spawn() -> Sender<Msg> {
    let (tx, rx) = channel::<Msg>();
    let _ = std::fs::create_dir_all(crate::app::data_dir());
    let _ = std::fs::write(
        crate::app::data_dir().join("discord.log"),
        format!("Rustify Discord RPC — appid {APP_ID}\n"),
    );
    std::thread::spawn(move || {
        let mut conn: Option<Box<dyn Pipe>> = None;
        let mut want: Option<Msg> = None;
        let mut nonce = 0u64;
        let mut last_err = String::new();
        loop {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(m) => {
                    let mut m = m;
                    while let Ok(n) = rx.try_recv() {
                        m = n;
                    }
                    want = Some(m);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            let Some(m) = &want else { continue };
            if conn.is_none() {
                match connect() {
                    Ok(c) => {
                        conn = Some(c);
                        last_err.clear();
                    }
                    Err(e) => {
                        if e != last_err {
                            log(&e);
                            last_err = e;
                        }
                    }
                }
            }
            let ok = match conn.as_mut() {
                Some(c) => deliver(
                    c.as_mut(),
                    &mut nonce,
                    match m {
                        Msg::Set(p) => Some(p),
                        Msg::Clear => None,
                    },
                ),
                None => false,
            };
            if ok {
                want = None;
            } else {
                conn = None;
                if matches!(want, Some(Msg::Clear)) {
                    want = None;
                }
            }
        }
    });
    tx
}
