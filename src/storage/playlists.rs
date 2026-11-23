use super::super::*;

pub(in crate::app) const MAX_RECENT: usize = 12;

pub(in crate::app) fn load_recent() -> Vec<String> {
    std::fs::read_to_string(user_dir().join("recent.txt"))
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .take(MAX_RECENT)
                .collect()
        })
        .unwrap_or_default()
}

pub(in crate::app) fn save_recent(v: &[String]) {
    let dir = user_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("recent.txt"), v.join("\n"));
}

pub(in crate::app) const MAX_HISTORY: usize = 20;

fn hit_json(h: &Hit) -> Value {
    json!({ "title": h.title, "artist": h.artist, "album": h.album, "secs": h.secs, "id": h.id, "thumb": h.thumb })
}

fn hit_from_json(v: &Value) -> Option<Hit> {
    let g = |k: &str| v[k].as_str().unwrap_or("").to_string();
    let id = g("id");
    if id.is_empty() {
        return None;
    }
    Some(Hit {
        title: g("title"),
        artist: g("artist"),
        album: g("album"),
        secs: v["secs"].as_u64().unwrap_or(0) as u32,
        id,
        thumb: g("thumb"),
    })
}

fn read_json(file: &str) -> Option<Vec<Value>> {
    let s = std::fs::read_to_string(user_dir().join(file)).ok()?;
    match serde_json::from_str::<Value>(&s) {
        Ok(Value::Array(a)) => Some(a),
        _ => None,
    }
}

fn write_json(file: &str, v: Value) {
    let dir = user_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(file), serde_json::to_string_pretty(&v).unwrap_or_default());
}

fn load_hits(file: &str, max: usize) -> Vec<Hit> {
    read_json(file)
        .map(|a| a.iter().filter_map(hit_from_json).take(max).collect())
        .unwrap_or_default()
}

fn save_hits(file: &str, v: &[Hit]) {
    write_json(file, Value::Array(v.iter().map(hit_json).collect()));
}

pub(in crate::app) fn load_history() -> Vec<Hit> {
    load_hits("history.json", MAX_HISTORY)
}

pub(in crate::app) fn save_history(v: &[Hit]) {
    save_hits("history.json", v);
}

pub(in crate::app) fn load_likes() -> Vec<Hit> {
    load_hits("likes.json", usize::MAX)
}

pub(in crate::app) fn save_likes(v: &[Hit]) {
    save_hits("likes.json", v);
}

#[derive(Clone)]
pub(in crate::app) struct Playlist {
    pub(in crate::app) name: String,
    pub(in crate::app) tracks: Vec<Hit>,
}

pub(in crate::app) fn load_playlists() -> Vec<Playlist> {
    read_json("playlists.json")
        .map(|a| {
            a.iter()
                .filter_map(|v| {
                    let name = v["name"].as_str()?.to_string();
                    let tracks = v["tracks"]
                        .as_array()
                        .map(|t| t.iter().filter_map(hit_from_json).collect())
                        .unwrap_or_default();
                    Some(Playlist { name, tracks })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(in crate::app) fn save_playlists(v: &[Playlist]) {
    let arr: Vec<Value> = v
        .iter()
        .map(|p| {
            let tracks: Vec<Value> = p.tracks.iter().map(hit_json).collect();
            json!({ "name": p.name, "tracks": tracks })
        })
        .collect();
    write_json("playlists.json", Value::Array(arr));
}
