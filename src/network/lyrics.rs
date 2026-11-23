use super::super::*;

#[derive(Clone)]
pub(in crate::app) struct Lyrics {
    pub(in crate::app) lines: Vec<(f32, String)>,
    pub(in crate::app) synced: bool,
    pub(in crate::app) instrumental: bool,
}

pub(in crate::app) enum LyRes {
    Found(Lyrics),
    NotFound,
    Error,
}

pub(in crate::app) enum LySlot {
    Loading(f64),
    Ready(Arc<Lyrics>),
    Missing,
    Error,
}

fn urlenc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => o.push(b as char),
            _ => o.push_str(&format!("%{b:02X}")),
        }
    }
    o
}

fn parse_lrc(s: &str) -> Vec<(f32, String)> {
    let mut out: Vec<(f32, String)> = Vec::new();
    for line in s.lines() {
        let mut rest = line.trim();
        let mut times: Vec<f32> = Vec::new();
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let mut it = rest[1..end].split(':');
            let (a, b) = (it.next(), it.next());
            let ok = match (
                a.and_then(|x| x.trim().parse::<f32>().ok()),
                b.and_then(|x| x.trim().parse::<f32>().ok()),
            ) {
                (Some(m), Some(sec)) if it.next().is_none() => {
                    times.push(m * 60.0 + sec);
                    true
                }
                _ => false,
            };
            if !ok {
                times.clear();
                rest = "";
                break;
            }
            rest = rest[end + 1..].trim_start();
        }
        if times.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for t in times {
            out.push((t, text.clone()));
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn clean_lyric(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !matches!(
                *c,
                '\u{FE0E}' | '\u{FE0F}' | '\u{200B}'..='\u{200D}' | '\u{2060}' | '\u{FEFF}'
            )
        })
        .collect()
}

fn tidy_lines(v: Vec<(f32, String)>) -> Vec<(f32, String)> {
    let mut out: Vec<(f32, String)> = Vec::new();
    for (t, s) in v {
        let s = clean_lyric(&s);
        if s.is_empty() && out.last().map_or(true, |(_, p)| p.is_empty()) {
            continue;
        }
        out.push((t, s));
    }
    while out.last().is_some_and(|(_, s)| s.is_empty()) {
        out.pop();
    }
    out
}

fn with_intro(mut v: Vec<(f32, String)>) -> Vec<(f32, String)> {
    if v.first().is_some_and(|(t, s)| *t > 3.0 && !s.is_empty()) {
        v.insert(0, (0.0, String::new()));
    }
    v
}

fn plain_to_lines(s: &str, dur: f32) -> Vec<(f32, String)> {
    let raw = tidy_lines(s.lines().map(|l| (0.0, l.trim().to_string())).collect());
    let dur = if dur > 20.0 { dur } else { 200.0 };
    let (a, b) = (5.0_f32, dur * 0.94);
    let n = raw.len().max(1) as f32;
    raw.into_iter()
        .enumerate()
        .map(|(i, (_, s))| (a + (b - a) * i as f32 / n, s))
        .collect()
}

fn lyrics_from_json(v: &Value, dur: f32) -> Option<Lyrics> {
    if v["instrumental"].as_bool() == Some(true) {
        return Some(Lyrics {
            lines: Vec::new(),
            synced: true,
            instrumental: true,
        });
    }
    if let Some(s) = v["syncedLyrics"].as_str() {
        let l = with_intro(tidy_lines(parse_lrc(s)));
        if !l.is_empty() {
            return Some(Lyrics {
                lines: l,
                synced: true,
                instrumental: false,
            });
        }
    }
    if let Some(s) = v["plainLyrics"].as_str() {
        let d = v["duration"]
            .as_f64()
            .map(|x| x as f32)
            .filter(|x| *x > 1.0)
            .unwrap_or(dur);
        let l = with_intro(plain_to_lines(s, d));
        if !l.is_empty() {
            return Some(Lyrics {
                lines: l,
                synced: false,
                instrumental: false,
            });
        }
    }
    None
}

fn clean_title(t: &str) -> String {
    let (mut out, mut depth) = (String::new(), 0i32);
    for ch in t.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let base = out.split(" - ").next().unwrap_or("").trim().to_string();
    if base.is_empty() {
        t.trim().to_string()
    } else {
        base
    }
}

fn first_artist(a: &str) -> String {
    let mut s = a.to_string();
    for sep in [" & ", ", ", " x ", " feat. ", " feat ", " ft. ", " with ", "/", " × "] {
        if let Some(i) = s.find(sep) {
            s.truncate(i);
        }
    }
    s.trim().to_string()
}

fn lrclib_json(url: &str) -> Result<Option<Value>, String> {
    match ureq::get(url)
        .header("User-Agent", "Rustify/0.1 (personal music player)")
        .call()
    {
        Ok(mut r) => r.body_mut().read_json::<Value>().map(Some).map_err(|e| e.to_string()),
        Err(e) => {
            let s = e.to_string();
            if s.contains("404") {
                Ok(None)
            } else {
                Err(s)
            }
        }
    }
}

fn pick_best(arr: &[Value], secs: u32) -> Option<Lyrics> {
    let dur = secs as f32;
    let mut best: Option<(i32, Lyrics)> = None;
    for v in arr {
        let d = v["duration"].as_f64().unwrap_or(0.0) as f32;
        let diff = (d - dur).abs();
        if secs > 0 && d > 0.0 && diff > 6.0 {
            continue;
        }
        let Some(l) = lyrics_from_json(v, dur) else { continue };
        let mut score = if l.synced && !l.instrumental { 20 } else { 0 };
        if secs > 0 && d > 0.0 {
            score += (10.0 - diff.min(10.0)) as i32;
        }
        if best.as_ref().map_or(true, |(s, _)| score > *s) {
            best = Some((score, l));
        }
    }
    best.map(|(_, l)| l)
}

fn lrclib_lookup(h: &Hit) -> LyRes {
    let dur = h.secs as f32;
    let get = |title: &str, artist: &str, album: &str| -> Result<Option<Lyrics>, String> {
        let mut url = format!(
            "https://lrclib.net/api/get?track_name={}&artist_name={}",
            urlenc(title),
            urlenc(artist)
        );
        if !album.is_empty() {
            url.push_str(&format!("&album_name={}", urlenc(album)));
        }
        if h.secs > 0 {
            url.push_str(&format!("&duration={}", h.secs));
        }
        Ok(lrclib_json(&url)?.and_then(|v| lyrics_from_json(&v, dur)))
    };

    match get(&h.title, &h.artist, &h.album) {
        Ok(Some(l)) => return LyRes::Found(l),
        Ok(None) => {}
        Err(_) => return LyRes::Error,
    }

    let (ct, fa) = (clean_title(&h.title), first_artist(&h.artist));
    if ct != h.title || fa != h.artist || !h.album.is_empty() {
        match get(&ct, &fa, "") {
            Ok(Some(l)) => return LyRes::Found(l),
            Ok(None) => {}
            Err(_) => return LyRes::Error,
        }
    }

    for url in [
        format!(
            "https://lrclib.net/api/search?track_name={}&artist_name={}",
            urlenc(&ct),
            urlenc(&fa)
        ),
        format!("https://lrclib.net/api/search?q={}", urlenc(&format!("{ct} {fa}"))),
    ] {
        match lrclib_json(&url) {
            Ok(Some(Value::Array(a))) => {
                if let Some(l) = pick_best(&a, h.secs) {
                    return LyRes::Found(l);
                }
            }
            Ok(_) => {}
            Err(_) => return LyRes::Error,
        }
    }
    LyRes::NotFound
}

fn lyrics_path(id: &str) -> std::path::PathBuf {
    data_dir().join("lyrics").join(format!("{id}.json"))
}

fn lyrics_disk_load(id: &str) -> Option<LyRes> {
    let path = lyrics_path(id);
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    if v["none"].as_bool() == Some(true) {
        let old = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.elapsed().ok())
            .map_or(true, |e| e.as_secs() > 3 * 86400);
        return if old { None } else { Some(LyRes::NotFound) };
    }
    let lines: Vec<(f32, String)> = v["lines"]
        .as_array()?
        .iter()
        .filter_map(|x| Some((x[0].as_f64()? as f32, clean_lyric(x[1].as_str()?))))
        .collect();
    Some(LyRes::Found(Lyrics {
        lines: with_intro(lines),
        synced: v["synced"].as_bool().unwrap_or(true),
        instrumental: v["instrumental"].as_bool().unwrap_or(false),
    }))
}

fn lyrics_disk_save(id: &str, r: &LyRes) {
    let _ = std::fs::create_dir_all(data_dir().join("lyrics"));
    let v = match r {
        LyRes::Found(l) => json!({
            "synced": l.synced,
            "instrumental": l.instrumental,
            "lines": l.lines.iter().map(|(t, s)| json!([t, s])).collect::<Vec<_>>(),
        }),
        _ => json!({ "none": true }),
    };
    let _ = std::fs::write(lyrics_path(id), v.to_string());
}

pub(in crate::app) fn fetch_lyrics(h: &Hit) -> LyRes {
    if let Some(r) = lyrics_disk_load(&h.id) {
        return r;
    }
    let r = lrclib_lookup(h);
    if !matches!(r, LyRes::Error) {
        lyrics_disk_save(&h.id, &r);
    }
    r
}
