use crate::discord;
use crate::i18n::{draw_language_flag, tr, Language};
use crate::profile::{Profile, PHOTO_PX};
use eframe::egui::{self, *};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicUsize, Ordering as AtOrd};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

#[path = "audio/mod.rs"]
mod audio;
use self::audio::Audio;
#[path = "network/mod.rs"]
mod network;
use self::network::{fetch_lyrics, yt_search, Hit, LyRes, LySlot, SearchMsg};
#[path = "storage/mod.rs"]
mod storage;
use self::storage::{
    load_discord_pref, load_history, load_language, load_likes, load_playlists, load_profiles, load_recent,
    migrate_legacy, new_profile_id, profiles_dir, save_discord_pref, save_history, save_language, save_likes,
    save_playlists, save_profiles, save_recent, set_user, user_dir, wipe_guest, Playlist, MAX_HISTORY, MAX_RECENT,
};
#[path = "ui/mod.rs"]
mod ui;
use self::ui::style::*;

struct Loaded {
    img: ColorImage,
    blur: ColorImage,
    avg: Color32,
}

enum Slot {
    Loading,
    Ready {
        tex: TextureHandle,
        blur: TextureHandle,
        avg: Color32,
    },
    Failed,
}

fn analyze(ci: &ColorImage) -> (ColorImage, Color32) {
    const N: usize = 48;
    let [w, h] = ci.size;
    if w == 0 || h == 0 {
        return (ColorImage::new([1, 1], BG), BG);
    }

    let (mut sr, mut sg, mut sb, mut sw) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for px in ci.pixels.iter().step_by(2) {
        let (r, g, b) = (px.r() as f64, px.g() as f64, px.b() as f64);
        let wgt = 1.0 + 3.0 * (r.max(g).max(b) - r.min(g).min(b)) / 255.0;
        sr += r * wgt;
        sg += g * wgt;
        sb += b * wgt;
        sw += wgt;
    }
    let avg = if sw > 0.0 {
        c((sr / sw) as u8, (sg / sw) as u8, (sb / sw) as u8)
    } else {
        BG
    };

    let mut small = vec![[0.0f32; 3]; N * N];
    for y in 0..N {
        let (y0, y1) = (y * h / N, ((y + 1) * h / N).max(y * h / N + 1).min(h));
        for x in 0..N {
            let (x0, x1) = (x * w / N, ((x + 1) * w / N).max(x * w / N + 1).min(w));
            let (mut a, mut n) = ([0.0f32; 3], 0.0f32);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let px = ci.pixels[yy * w + xx];
                    a[0] += px.r() as f32;
                    a[1] += px.g() as f32;
                    a[2] += px.b() as f32;
                    n += 1.0;
                }
            }
            if n > 0.0 {
                small[y * N + x] = [a[0] / n, a[1] / n, a[2] / n];
            }
        }
    }

    for _ in 0..3 {
        let src = small.clone();
        for y in 0..N {
            for x in 0..N {
                let mut a = [0.0f32; 3];
                let mut n = 0.0f32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (xx, yy) = (
                            (x as i32 + dx).clamp(0, N as i32 - 1) as usize,
                            (y as i32 + dy).clamp(0, N as i32 - 1) as usize,
                        );
                        let v = src[yy * N + xx];
                        a[0] += v[0];
                        a[1] += v[1];
                        a[2] += v[2];
                        n += 1.0;
                    }
                }
                small[y * N + x] = [a[0] / n, a[1] / n, a[2] / n];
            }
        }
    }
    let mut rgba = Vec::with_capacity(N * N * 4);
    for v in &small {
        rgba.extend_from_slice(&[v[0] as u8, v[1] as u8, v[2] as u8, 255]);
    }
    (ColorImage::from_rgba_unmultiplied([N, N], &rgba), avg)
}

fn readable(bg: Color32) -> Color32 {
    let lin = |v: u8| {
        let x = v as f32 / 255.0;
        if x <= 0.03928 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    let lum = |col: Color32| 0.2126 * lin(col.r()) + 0.7152 * lin(col.g()) + 0.0722 * lin(col.b());
    let ratio = |a: f32, b: f32| (a.max(b) + 0.05) / (a.min(b) + 0.05);
    let (dark, light) = (c(18, 18, 20), TEXT);
    let l = lum(bg);
    if ratio(l, lum(dark)) >= ratio(l, lum(light)) {
        dark
    } else {
        light
    }
}

fn hero_image(p: &Painter, r: Rect, rd: f32, tex: TextureId, x0: f32, y0: f32, s: f32) {
    use std::f32::consts::FRAC_PI_2;

    let (n, nc) = (24usize, 14usize);
    let flat_end = r.right() - rd;
    let mut cols: Vec<(f32, f32, f32, f32)> = Vec::new();
    for i in 0..=n {
        cols.push((x0 + (flat_end - x0) * i as f32 / n as f32, 0.0, 0.0, 1.0));
    }
    for j in 1..=nc {
        let a = FRAC_PI_2 * j as f32 / nc as f32;
        cols.push((flat_end + rd * a.sin(), rd * (1.0 - a.cos()), a.sin(), a.cos()));
    }
    let uv = |q: Pos2| pos2((q.x - x0) / s, (q.y - y0) / s);
    let mut m = Mesh::with_texture(tex);
    for &(x, inset, nx, ny) in &cols {
        let t = ((x - x0) / (r.right() - x0)).clamp(0.0, 1.0);
        let col = Color32::WHITE.gamma_multiply(t * t * (3.0 - 2.0 * t) * 0.80);
        let (top, bot) = (pos2(x, r.top() + inset), pos2(x, r.bottom() - inset));
        let (ot, ob) = (top + vec2(nx, -ny), bot + vec2(nx, ny));
        for (q, c) in [
            (ot, Color32::TRANSPARENT),
            (top, col),
            (bot, col),
            (ob, Color32::TRANSPARENT),
        ] {
            m.vertices.push(egui::epaint::Vertex {
                pos: q,
                uv: uv(q),
                color: c,
            });
        }
    }
    for i in 0..cols.len() - 1 {
        let (a, b) = (4 * i as u32, 4 * (i as u32 + 1));
        for k in 0..3u32 {
            m.add_triangle(a + k, a + k + 1, b + k);
            m.add_triangle(a + k + 1, b + k + 1, b + k);
        }
    }
    p.add(Shape::mesh(m));
}

struct ImgCache {
    slots: RefCell<HashMap<String, Slot>>,
    req: Sender<String>,
    done: Receiver<(String, Option<Loaded>)>,
}

impl ImgCache {
    fn new(ctx: &Context) -> Self {
        let (req, req_rx) = channel::<String>();
        let req_rx = Arc::new(Mutex::new(req_rx));
        let (done_tx, done) = channel();
        for _ in 0..4 {
            let (rx, tx, ctx) = (req_rx.clone(), done_tx.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                let url = match rx.lock().unwrap().recv() {
                    Ok(u) => u,
                    Err(_) => break,
                };
                let img = load_image(&url).map(|ci| {
                    let (blur, avg) = analyze(&ci);
                    Loaded { img: ci, blur, avg }
                });
                let _ = tx.send((url, img));
                ctx.request_repaint();
            });
        }
        std::thread::spawn(prune_cache);
        Self {
            slots: RefCell::new(HashMap::new()),
            req,
            done,
        }
    }

    fn poll(&self, ctx: &Context) {
        let mut s = self.slots.borrow_mut();
        while let Ok((url, img)) = self.done.try_recv() {
            let slot = match img {
                Some(l) => Slot::Ready {
                    tex: ctx.load_texture(url.clone(), l.img, TextureOptions::LINEAR),
                    blur: ctx.load_texture(format!("{url}#blur"), l.blur, TextureOptions::LINEAR),
                    avg: l.avg,
                },
                None => Slot::Failed,
            };
            s.insert(url, slot);
        }
    }

    fn get(&self, url: &str) -> Option<TextureHandle> {
        self.get_full(url).map(|(t, _, _)| t)
    }

    fn get_full(&self, url: &str) -> Option<(TextureHandle, TextureHandle, Color32)> {
        let mut s = self.slots.borrow_mut();
        match s.get(url) {
            Some(Slot::Ready { tex, blur, avg }) => Some((tex.clone(), blur.clone(), *avg)),
            Some(_) => None,
            None => {
                s.insert(url.to_string(), Slot::Loading);
                let _ = self.req.send(url.to_string());
                None
            }
        }
    }
}

fn cache_path(url: &str) -> std::path::PathBuf {
    let h = url
        .bytes()
        .fold(0xcbf29ce484222325u64, |a, b| (a ^ b as u64).wrapping_mul(0x100000001b3));
    data_dir().join("cache").join(format!("{h:016x}.img"))
}

fn load_image(url: &str) -> Option<ColorImage> {
    let path = cache_path(url);
    let (bytes, from_disk) = match std::fs::read(&path) {
        Ok(b) if !b.is_empty() => (b, true),
        _ => {
            let mut r = ureq::get(url).call().ok()?;
            (r.body_mut().read_to_vec().ok()?, false)
        }
    };
    match ::image::load_from_memory(&bytes) {
        Ok(img) => {
            if !from_disk {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::write(&path, &bytes);
            }
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
        }
        Err(_) => {
            if from_disk {
                let _ = std::fs::remove_file(&path);
            }
            None
        }
    }
}

fn prune_cache() {
    let Ok(rd) = std::fs::read_dir(data_dir().join("cache")) else {
        return;
    };
    let mut files: Vec<_> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= 600 {
        return;
    }
    files.sort();
    let n = files.len() - 450;
    for (_, p) in files.into_iter().take(n) {
        let _ = std::fs::remove_file(p);
    }
}

fn art(ui: &Ui, p: &Painter, cr: Rect, imgs: &ImgCache, urls: &[&str], seed: usize, rd: f32) {
    for u in urls.iter().filter(|u| !u.is_empty()) {
        if let Some(t) = imgs.get(u) {
            Image::from_texture(&t).rounding(rd).paint_at(ui, cr);
            return;
        }
    }
    cover(p, cr, seed, rd, false);
}

fn art_zoom(ui: &Ui, p: &Painter, cr: Rect, imgs: &ImgCache, urls: &[&str], seed: usize, rd: f32, t: f32) {
    let k = 0.5 / (1.0 + 0.10 * t);
    let uv = Rect::from_min_max(pos2(0.5 - k, 0.5 - k), pos2(0.5 + k, 0.5 + k));
    for u in urls.iter().filter(|u| !u.is_empty()) {
        if let Some(tx) = imgs.get(u) {
            Image::from_texture(&tx).rounding(rd).uv(uv).paint_at(ui, cr);
            return;
        }
    }
    cover(p, cr, seed, rd, false);
}

fn art_tile(ui: &Ui, p: &Painter, r: Rect, imgs: &ImgCache, urls: &[&str], seed: usize, rd: Rounding, z: f32) {
    for u in urls.iter().filter(|u| !u.is_empty()) {
        if let Some(t) = imgs.get(u) {
            let (w, h) = (r.width(), r.height());
            let uv = if w < h {
                let k = w / h;
                Rect::from_min_max(pos2(0.5 - k / 2.0, 0.0), pos2(0.5 + k / 2.0, 1.0))
            } else if h < w {
                let k = h / w;
                Rect::from_min_max(pos2(0.0, 0.5 - k / 2.0), pos2(1.0, 0.5 + k / 2.0))
            } else {
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
            };
            let uv = Rect::from_center_size(uv.center(), uv.size() * (1.0 / (1.0 + 0.10 * z)));
            Image::from_texture(&t).rounding(rd).uv(uv).paint_at(ui, r);
            return;
        }
    }
    p.rect_filled(r, rd, PAL[seed % PAL.len()][0]);
}

fn playlist_cover(ui: &Ui, p: &Painter, cr: Rect, imgs: &ImgCache, tracks: &[Hit], name: &str, z: f32) {
    const RD: f32 = 8.0;
    let last: Vec<&Hit> = tracks.iter().rev().take(4).collect();
    if last.is_empty() {
        cover(p, cr, seed_of(name), RD, false);
        return;
    }
    let g = 2.0;
    let (hw, hh) = ((cr.width() - g) / 2.0, (cr.height() - g) / 2.0);
    let q = |cx: usize, cy: usize| {
        Rect::from_min_size(
            pos2(cr.left() + cx as f32 * (hw + g), cr.top() + cy as f32 * (hh + g)),
            vec2(hw, hh),
        )
    };
    let half = |cx: usize| Rect::from_min_size(pos2(cr.left() + cx as f32 * (hw + g), cr.top()), vec2(hw, cr.height()));
    let rd = |nw: bool, ne: bool, sw: bool, se: bool| Rounding {
        nw: if nw { RD } else { 0.0 },
        ne: if ne { RD } else { 0.0 },
        sw: if sw { RD } else { 0.0 },
        se: if se { RD } else { 0.0 },
    };
    let tiles: Vec<(Rect, Rounding)> = match last.len() {
        1 => vec![(cr, rd(true, true, true, true))],
        2 => vec![
            (half(0), rd(true, false, true, false)),
            (half(1), rd(false, true, false, true)),
        ],
        3 => vec![
            (half(0), rd(true, false, true, false)),
            (q(1, 0), rd(false, true, false, false)),
            (q(1, 1), rd(false, false, false, true)),
        ],
        _ => vec![
            (q(0, 0), rd(true, false, false, false)),
            (q(1, 0), rd(false, true, false, false)),
            (q(0, 1), rd(false, false, true, false)),
            (q(1, 1), rd(false, false, false, true)),
        ],
    };
    for (t, (r, round)) in last.iter().zip(tiles) {
        let big = t.thumb.replace("w96-h96", "w300-h300");

        let urls: Vec<&str> = if last.len() == 4 {
            vec![t.thumb.as_str()]
        } else {
            vec![big.as_str(), t.thumb.as_str()]
        };
        art_tile(ui, p, r, imgs, &urls, seed_of(&t.id), round, z);
    }
}

fn wrapped(p: &Painter, pos: Pos2, text: &str, font: FontId, col: Color32, w: f32, rows: usize) -> f32 {
    let mut job = text::LayoutJob::simple(text.to_string(), font, col, w);
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = true;
    let g = p.layout_job(job);
    let h = g.size().y;
    p.galley(pos, g, col);
    h
}

fn layout_words(
    p: &Painter,
    text: &str,
    font: FontId,
    col: Color32,
    w: f32,
    rows: usize,
) -> std::sync::Arc<egui::Galley> {
    let mut job = text::LayoutJob::simple(text.to_string(), font, col, w);
    job.wrap.max_rows = rows;
    job.wrap.break_anywhere = false;
    p.layout_job(job)
}

pub(crate) fn data_dir() -> std::path::PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("Rustify")
}

fn taste_query(t: usize) -> String {
    format!("{} hits", GENRE_TERMS[t])
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

fn seed_of(s: &str) -> usize {
    s.bytes()
        .fold(7usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize))
}

pub(crate) const GENRES: [&str; 12] = [
    "Pop",
    "Hip-Hop",
    "Electronic",
    "Rock",
    "Jazz",
    "Chill",
    "Classical",
    "Indie",
    "Soul",
    "Latin",
    "Metal",
    "K-Pop",
];

fn genre_name(lang: Language, index: usize) -> &'static str {
    match lang {
        Language::Fr => match index {
            2 => "Électro",
            6 => "Classique",
            9 => "Latino",
            _ => GENRES[index],
        },
        Language::En => match index {
            2 => "Electronic",
            6 => "Classical",
            9 => "Latin",
            _ => GENRES[index],
        },
        Language::It => match index {
            2 => "Elettronica",
            6 => "Classica",
            9 => "Latino",
            _ => match index {
                5 => "Relax",
                10 => "Metal",
                11 => "K-Pop",
                _ => GENRES[index],
            },
        },
        Language::Es => match index {
            2 => "Electrónica",
            6 => "Clásica",
            9 => "Latino",
            _ => match index {
                5 => "Relajante",
                10 => "Metal",
                11 => "K-Pop",
                _ => GENRES[index],
            },
        },
        Language::Zh => match index {
            0 => "流行",
            1 => "嘻哈",
            2 => "电子",
            3 => "摇滚",
            4 => "爵士",
            5 => "轻松",
            6 => "古典",
            7 => "独立",
            8 => "灵魂",
            9 => "拉丁",
            10 => "金属",
            11 => "韩流",
            _ => "",
        },
        Language::Ru => match index {
            0 => "Поп",
            1 => "Хип-хоп",
            2 => "Электроника",
            3 => "Рок",
            4 => "Джаз",
            5 => "Релакс",
            6 => "Классика",
            7 => "Инди",
            8 => "Соул",
            9 => "Латино",
            10 => "Метал",
            11 => "K-Pop",
            _ => "",
        },
    }
}

const GENRE_TERMS: [&str; 12] = [
    "pop music",
    "hip hop",
    "electronic music",
    "rock",
    "jazz",
    "chill music",
    "classical music",
    "indie",
    "soul music",
    "latin music",
    "metal",
    "k-pop",
];

const MOOD_QUERIES: [&str; 6] = [
    "",
    "focus music instrumental",
    "chill music",
    "energy workout music",
    "night drive music",
    "summer vibes music",
];
const TOP_QUERY: &str = "top hits 2026";

const MINI_W: f32 = 424.0;
const MINI_H: f32 = 136.0;
const TB_H: f32 = 36.0;
const WIN_R: f32 = 8.0;

#[derive(PartialEq, Clone, Copy)]
enum View {
    Home,
    Search,
    Library,
}

#[derive(PartialEq, Clone, Copy)]
enum Row {
    Open,
    Like,
    Add,
    Remove,
}

const HEART_DX: f32 = 62.0;
const ADD_DX: f32 = 94.0;
const RM_DX: f32 = 126.0;

#[derive(Clone)]
struct PlMenu {
    hit: Hit,
    at: Pos2,
}

#[derive(PartialEq, Clone, Copy)]
enum PlEdit {
    New,
    Rename(usize),
}

#[derive(PartialEq, Clone, Copy)]
enum Pui {
    Picker,
    Form,
    Panel,
    Off,
}

struct Form {
    edit: Option<usize>,
    name: String,
    tastes: [bool; 12],
    avatar: Option<usize>,
    photo: Option<(::image::RgbaImage, TextureHandle)>,
    keep_photo: bool,
    del: bool,
    focus: bool,
    step: usize,
    goto: usize,
    dir: f32,
    phase: u8,
    t0: f64,
    caret: Caret,
    err: Option<&'static str>,
}

enum FormAct {
    Cancel,
    Save,
    Delete,
}

fn quiet_rodio_panics() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info
            .location()
            .is_some_and(|l| l.file().contains("rodio") || l.file().contains("symphonia"))
        {
            eprintln!("[decoder panic] {info}");
        } else {
            prev(info);
        }
    }));
}

static HOVER: Mutex<(String, f64, bool)> = Mutex::new((String::new(), 0.0, false));

fn hover_prefetch(ui: &Ui, id: &str, hovered: bool) {
    if !hovered {
        return;
    }
    let now = ui.input(|i| i.time);
    let mut st = HOVER.lock().unwrap_or_else(|e| e.into_inner());
    if st.0 != id {
        *st = (id.to_string(), now, false);
    }
    if st.2 {
        return;
    }
    if now - st.1 >= 0.35 {
        st.2 = true;
        drop(st);
        cache_enqueue(&[id.to_string()], true);
    } else {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(400));
    }
}

fn ytdlp_path() -> std::path::PathBuf {
    let name = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
    let mut cands = vec![data_dir().join(name)];
    if let Some(d) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
    {
        cands.insert(0, d.join(name));
    }
    cands.into_iter().find(|p| p.exists()).unwrap_or_else(|| name.into())
}

#[derive(Clone, PartialEq)]
enum Dep {
    Idle,
    Ask { ytdlp: bool, ffmpeg: bool },
    Installing,
    Done,
    Failed(String),
}

struct DepProgress {
    label: String,
    frac: f32,
    result: Option<Result<(), String>>,
}

fn tool_ok(path: &std::path::Path, arg: &str) -> bool {
    let mut cmd = std::process::Command::new(path);
    cmd.arg(arg)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.status().map_or(false, |s| s.success())
}

fn missing_tools() -> (bool, bool) {
    (
        !tool_ok(&ytdlp_path(), "--version"),
        !tool_ok(&ffmpeg_path(), "-version"),
    )
}

#[cfg(windows)]
const FFMPEG_URLS: [&str; 2] = [
    "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
];

#[cfg(windows)]
fn download_file(url: &str, dest: &std::path::Path, mut prog: impl FnMut(f32)) -> Result<(), String> {
    use std::io::{Read, Write};
    let mut resp = ureq::get(url)
        .header("User-Agent", "Rustify/0.1 (personal music player)")
        .call()
        .map_err(|e| format!("Download failed ({url}): {e}"))?;
    let total = resp.body().content_length();
    let mut rd = resp.body_mut().as_reader();
    let tmp = dest.with_extension("part");
    let mut f = std::fs::File::create(&tmp).map_err(|e| format!("{} : {e}", tmp.display()))?;
    let mut buf = vec![0u8; 64 * 1024];
    let (mut done, mut last) = (0u64, std::time::Instant::now());
    loop {
        let n = rd.read(&mut buf).map_err(|e| format!("Download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n]).map_err(|e| format!("Write failed: {e}"))?;
        done += n as u64;
        if last.elapsed().as_millis() >= 60 {
            last = std::time::Instant::now();
            prog(total.map_or(0.0, |t| (done as f32 / t as f32).min(1.0)));
        }
    }
    f.flush().map_err(|e| e.to_string())?;
    drop(f);
    if total.is_some_and(|t| t != done) {
        let _ = std::fs::remove_file(&tmp);
        return Err("Download was incomplete".to_string());
    }
    prog(1.0);
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&tmp, dest).map_err(|e| format!("{} : {e}", dest.display()))
}

#[cfg(windows)]
fn find_file(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_file(&p, name) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            return Some(p);
        }
    }
    None
}

#[cfg(windows)]
fn extract_ffmpeg(zip: &std::path::Path, dir: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let tmp = dir.join("ffmpeg_tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("{} : {e}", tmp.display()))?;
    let tar = std::env::var_os("SystemRoot")
        .map(|r| std::path::PathBuf::from(r).join("System32").join("tar.exe"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| "tar".into());
    let mut found = None;
    for attempt in 0..3 {
        let mut c = match attempt {
            0 => {
                let mut c = Command::new(&tar);
                c.arg("-xf").arg(zip).arg("-C").arg(&tmp).arg("*/bin/ffmpeg.exe");
                c
            }
            1 => {
                let mut c = Command::new(&tar);
                c.arg("-xf").arg(zip).arg("-C").arg(&tmp);
                c
            }
            _ => {
                let q = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
                let mut c = Command::new("powershell");
                c.args(["-NoProfile", "-NonInteractive", "-Command"]).arg(format!(
                    "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                    q(zip),
                    q(&tmp)
                ));
                c
            }
        };
        let _ = c
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        found = find_file(&tmp, "ffmpeg.exe");
        if found.is_some() {
            break;
        }
    }
    let src = found.ok_or_else(|| "Could not extract ffmpeg.exe from the archive".to_string())?;
    let dst = dir.join("ffmpeg.exe");
    let _ = std::fs::remove_file(&dst);
    std::fs::rename(&src, &dst)
        .or_else(|_| std::fs::copy(&src, &dst).map(|_| ()))
        .map_err(|e| format!("ffmpeg.exe : {e}"))?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

#[cfg(windows)]
fn install_tools(
    ytdlp: bool,
    ffmpeg: bool,
    st: &Mutex<DepProgress>,
    ctx: &Context,
    lang: Language,
) -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let w_y = if ytdlp { 12.0_f32 } else { 0.0 };
    let w_d = if ffmpeg { 78.0_f32 } else { 0.0 };
    let w_x = if ffmpeg { 10.0_f32 } else { 0.0 };
    let total = w_y + w_d + w_x;
    let set = |label: &str, done: f32| {
        {
            let mut g = st.lock().unwrap_or_else(|e| e.into_inner());
            g.label = label.to_string();
            g.frac = (done / total).clamp(0.0, 1.0);
        }
        ctx.request_repaint();
    };
    if ytdlp {
        set(tr(lang, "downloading_ytdlp"), 0.0);
        download_file(
            "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
            &dir.join("yt-dlp.exe"),
            |f| set(tr(lang, "downloading_ytdlp"), w_y * f),
        )?;
    }
    if ffmpeg {
        let zip = dir.join("ffmpeg.zip");
        let mut last_err = String::new();
        let mut got = false;
        for url in FFMPEG_URLS {
            set(tr(lang, "downloading_ffmpeg"), w_y);
            match download_file(url, &zip, |f| set(tr(lang, "downloading_ffmpeg"), w_y + w_d * f)) {
                Ok(()) => {
                    got = true;
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        if !got {
            return Err(last_err);
        }
        set(tr(lang, "extracting_ffmpeg"), w_y + w_d);
        let r = extract_ffmpeg(&zip, &dir);
        let _ = std::fs::remove_file(&zip);
        r?;
    }
    set(tr(lang, "verifying"), total);
    let (y, f) = missing_tools();
    if ytdlp && y {
        return Err(tr(lang, "ytdlp_start_error").to_string());
    }
    if ffmpeg && f {
        return Err(tr(lang, "ffmpeg_start_error").to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
fn install_tools(
    _ytdlp: bool,
    _ffmpeg: bool,
    _st: &Mutex<DepProgress>,
    _ctx: &Context,
    _lang: Language,
) -> Result<(), String> {
    Err(
        "Automatic installation is only available on Windows; install yt-dlp and ffmpeg with your package manager."
            .to_string(),
    )
}

const RECENT_CAP: u64 = 600 * 1024 * 1024;
const HISTORY_KEEP: usize = 30;

#[derive(Default)]
struct CacheState {
    files: HashMap<String, std::path::PathBuf>,
    queue: std::collections::VecDeque<String>,
    inflight: std::collections::HashSet<String>,
    failed: std::collections::HashSet<String>,
    protected: std::collections::HashSet<String>,
}

static CACHE: std::sync::OnceLock<(Mutex<CacheState>, std::sync::Condvar)> = std::sync::OnceLock::new();
static URGENT: AtomicUsize = AtomicUsize::new(0);
static INIT_EVICT: std::sync::Once = std::sync::Once::new();

fn audio_dir() -> std::path::PathBuf {
    data_dir().join("audio")
}

fn is_audio(p: &std::path::Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e, "ogg" | "m4a" | "mp4" | "aac" | "mp3"))
}

fn cache() -> &'static (Mutex<CacheState>, std::sync::Condvar) {
    CACHE.get_or_init(|| {
        let mut st = CacheState::default();
        if let Ok(rd) = std::fs::read_dir(audio_dir()) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() && is_audio(&p) {
                    if let Some(id) = p.file_stem().and_then(|s| s.to_str()) {
                        st.files.insert(id.to_string(), p.clone());
                    }
                }
            }
        }
        (Mutex::new(st), std::sync::Condvar::new())
    })
}

fn cache_lock() -> std::sync::MutexGuard<'static, CacheState> {
    cache().0.lock().unwrap_or_else(|e| e.into_inner())
}

fn cache_init() {
    let _ = std::fs::remove_dir_all(audio_dir().join("tmp"));
    cache();
    std::thread::spawn(prefetch_worker);
}

fn audio_cached(id: &str) -> Option<std::path::PathBuf> {
    let p = cache_lock().files.get(id).cloned()?;
    if p.exists() {
        Some(p)
    } else {
        cache_lock().files.remove(id);
        None
    }
}

fn touch(p: &std::path::Path) {
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(p) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
}

fn cache_enqueue(ids: &[String], front: bool) {
    let mut st = cache_lock();
    let mut add: Vec<String> = Vec::new();
    for id in ids {
        if st.files.contains_key(id) || st.inflight.contains(id) || st.failed.contains(id) || add.contains(id) {
            continue;
        }
        if !front && st.queue.contains(id) {
            continue;
        }
        add.push(id.clone());
    }
    if add.is_empty() {
        return;
    }
    if front {
        st.queue.retain(|q| !add.contains(q));
        for id in add.into_iter().rev() {
            st.queue.push_front(id);
        }
    } else {
        st.queue.extend(add);
    }
    cache().1.notify_one();
}

fn download(id: &str) -> Result<std::path::PathBuf, String> {
    loop {
        if let Some(p) = audio_cached(id) {
            return Ok(p);
        }
        if cache_lock().inflight.insert(id.to_string()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
    let t0 = std::time::Instant::now();
    let r = ytdlp_fetch(id);
    eprintln!("[cache] {id}: download completed in {:.1}s", t0.elapsed().as_secs_f32());
    let mut st = cache_lock();
    st.inflight.remove(id);
    if let Ok(p) = &r {
        st.files.insert(id.to_string(), p.clone());
    }
    r
}

fn fetch_audio(id: &str) -> Result<std::path::PathBuf, String> {
    if let Some(p) = audio_cached(id) {
        touch(&p);
        return Ok(p);
    }
    let t0 = std::time::Instant::now();
    URGENT.fetch_add(1, AtOrd::Relaxed);
    let r = download(id);
    URGENT.fetch_sub(1, AtOrd::Relaxed);
    eprintln!("[audio] {id} downloaded in {:.1}s", t0.elapsed().as_secs_f32());
    r
}

fn run_ytdlp(id: &str, tmp: &std::path::Path, args: &[&str]) -> Result<std::path::PathBuf, String> {
    if let Some(old) = find_any(tmp, id) {
        let _ = std::fs::remove_file(old);
    }
    let mut cmd = std::process::Command::new(ytdlp_path());
    let ff = ffmpeg_path();
    if ff.is_absolute() && ff.exists() {
        cmd.arg("--ffmpeg-location").arg(&ff);
    }
    cmd.args(args)
        .args([
            "--no-playlist",
            "--no-part",
            "--no-warnings",
            "--concurrent-fragments",
            "4",
            "-q",
            "-o",
        ])
        .arg(tmp.join("%(id)s.%(ext)s"))
        .arg(format!("https://www.youtube.com/watch?v={id}"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let t0 = std::time::Instant::now();
    let out = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "yt-dlp was not found; place yt-dlp.exe next to the app or add it to PATH".to_string()
        } else {
            format!("yt-dlp : {e}")
        }
    })?;
    eprintln!(
        "[yt-dlp] {id} {:?} : {:.1} s, {}",
        args,
        t0.elapsed().as_secs_f32(),
        if out.status.success() { "ok" } else { "failed" }
    );
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        eprintln!(
            "[yt-dlp] {}",
            err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("")
        );
        let line = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("download failed");
        if err.to_lowercase().contains("ffmpeg") || err.to_lowercase().contains("ffprobe") {
            return Err(
                "ffmpeg was not found (required for audio conversion); add it to PATH or place it next to yt-dlp.exe"
                    .to_string(),
            );
        }
        return Err(line.trim().to_string());
    }
    find_any(tmp, id).ok_or_else(|| "Audio file was not found after download".to_string())
}

fn find_any(dir: &std::path::Path, id: &str) -> Option<std::path::PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(id))
}

fn ffmpeg_path() -> std::path::PathBuf {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Some(d) = ytdlp_path().parent() {
        dirs.push(d.to_path_buf());
    }
    if let Some(d) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.to_path_buf()))
    {
        dirs.push(d);
    }
    dirs.push(data_dir());
    dirs.into_iter()
        .map(|d| d.join(name))
        .find(|p| p.exists())
        .unwrap_or_else(|| name.into())
}

fn to_ogg(src: &std::path::Path, id: &str, tmp: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let dst = tmp.join(format!("{id}.ogg"));
    let _ = std::fs::remove_file(&dst);
    let t0 = std::time::Instant::now();
    let mut cmd = std::process::Command::new(ffmpeg_path());
    cmd.args(["-y", "-v", "error", "-i"])
        .arg(src)
        .args(["-vn", "-c:a", "libvorbis", "-q:a", "4", "-threads", "0"])
        .arg(&dst);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "ffmpeg was not found (required for audio conversion); add it to PATH or place it next to yt-dlp.exe"
                .to_string()
        } else {
            format!("ffmpeg : {e}")
        }
    })?;
    eprintln!("[ffmpeg] {id} → ogg : {:.1} s", t0.elapsed().as_secs_f32());
    if !out.status.success() || !dst.exists() {
        let _ = std::fs::remove_file(&dst);
        return Err(String::from_utf8_lossy(&out.stderr)
            .lines()
            .last()
            .unwrap_or("conversion failed")
            .to_string());
    }
    let _ = std::fs::remove_file(src);
    Ok(dst)
}

fn ytdlp_fetch(id: &str) -> Result<std::path::PathBuf, String> {
    let dir = audio_dir();
    let tmp = dir.join("tmp");
    let _ = std::fs::create_dir_all(&tmp);
    let t0 = std::time::Instant::now();
    let raw = match run_ytdlp(id, &tmp, &["-f", "ba[ext=m4a]/ba"]) {
        Err(e) if e.contains("403") => run_ytdlp(id, &tmp, &["-f", "ba[ext=m4a]/ba"])?,
        r => r?,
    };
    let t1 = t0.elapsed().as_secs_f32();
    let src = to_ogg(&raw, id, &tmp)?;
    eprintln!(
        "[cache] {id} : yt-dlp {t1:.1} s + conversion {:.1} s",
        t0.elapsed().as_secs_f32() - t1
    );
    let dst = dir.join(src.file_name().unwrap_or_default());
    if std::fs::rename(&src, &dst).is_err() {
        std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&src);
    }
    Ok(dst)
}

fn prefetch_worker() {
    loop {
        let id = {
            let mut st = cache_lock();
            loop {
                if let Some(id) = st.queue.pop_front() {
                    break id;
                }
                st = cache().1.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        };
        while URGENT.load(AtOrd::Relaxed) > 0 {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        if audio_cached(&id).is_some() {
            continue;
        }
        if download(&id).is_err() {
            cache_lock().failed.insert(id);
        } else {
            cache_evict();
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
}

fn cache_evict() {
    let (protected, files) = {
        let st = cache_lock();
        (st.protected.clone(), st.files.clone())
    };
    let mut v: Vec<(String, std::path::PathBuf, u64, std::time::SystemTime)> = files
        .into_iter()
        .filter(|(id, _)| !protected.contains(id))
        .filter_map(|(id, p)| {
            let m = std::fs::metadata(&p).ok()?;
            Some((id, p, m.len(), m.modified().ok()?))
        })
        .collect();
    let mut total: u64 = v.iter().map(|x| x.2).sum();
    if total <= RECENT_CAP {
        return;
    }
    v.sort_by_key(|x| x.3);
    for (id, p, len, _) in v {
        if total <= RECENT_CAP {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total = total.saturating_sub(len);
            cache_lock().files.remove(&id);
        }
    }
}

fn rand_below(n: usize) -> usize {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0) as usize;
    (t.wrapping_mul(2654435761) >> 7) % n.max(1)
}

#[allow(unreachable_code)]
fn local_hour() -> u32 {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime {
            y: u16,
            mo: u16,
            dow: u16,
            d: u16,
            h: u16,
            mi: u16,
            s: u16,
            ms: u16,
        }
        extern "system" {
            fn GetLocalTime(st: *mut SystemTime);
        }
        let mut st = SystemTime::default();
        unsafe { GetLocalTime(&mut st) };
        return (st.h as u32).min(23);
    }
    #[cfg(unix)]
    {
        use std::ffi::{c_char, c_int, c_long};
        #[repr(C)]
        struct Tm {
            sec: c_int,
            min: c_int,
            hour: c_int,
            mday: c_int,
            mon: c_int,
            year: c_int,
            wday: c_int,
            yday: c_int,
            isdst: c_int,
            gmtoff: c_long,
            zone: *const c_char,
        }
        extern "C" {
            fn time(t: *mut c_long) -> c_long;
            fn localtime_r(t: *const c_long, out: *mut Tm) -> *mut Tm;
        }
        unsafe {
            let t = time(std::ptr::null_mut());
            let mut tm: Tm = std::mem::zeroed();
            if !localtime_r(&t, &mut tm).is_null() {
                return (tm.hour as u32).min(23);
            }
        }
    }

    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    ((s / 3600) % 24) as u32
}

fn day_slot(hour: u32) -> usize {
    match hour {
        5..=7 => 0,
        8..=11 => 1,
        12..=13 => 2,
        14..=17 => 3,
        18..=21 => 4,
        _ => 5,
    }
}

const GREETINGS: [&[(&str, &str)]; 6] = [
    &[
        ("BIEN LEVÉ, {n}", "Un petit son pour bien démarrer ?"),
        ("BONJOUR", "Qu'est-ce qu'on écoute pour se réveiller ?"),
        ("DÉJÀ DEBOUT, {n} ?", "Un café, un morceau ?"),
        ("LE JOUR SE LÈVE", "Quelle musique pour accueillir l'aube ?"),
        ("{n}, LE MATIN EST À TOI", "On commence tout en douceur ?"),
        ("TÔT, MAIS EN FORME", "Un peu d'énergie pour ouvrir la journée ?"),
    ],
    &[
        ("BONJOUR, {n}", "Qu'est-ce qu'on écoute ?"),
        ("BONNE MATINÉE", "Une bande-son pour bien attaquer la journée ?"),
        ("SALUT {n}", "Un morceau pour se mettre en route ?"),
        ("LA MATINÉE COMMENCE", "Focus ou bonne humeur ?"),
        ("{n}, C'EST PARTI", "Quel son pour ce matin ?"),
        ("HELLO", "Prêt pour une bonne playlist ?"),
    ],
    &[
        ("BON APPÉTIT, {n}", "Une musique pour la pause déj ?"),
        ("C'EST L'HEURE DE LA PAUSE", "Qu'est-ce qu'on met pendant le repas ?"),
        ("À MIDI", "Un petit air pour accompagner l'assiette ?"),
        ("{n}, PAUSE MÉRITÉE", "Un son tranquille pour digérer ?"),
        ("BONNE PAUSE", "On se détend un peu ?"),
        ("MI-JOURNÉE", "Quel morceau pour recharger les batteries ?"),
    ],
    &[
        ("BON APRÈS-MIDI, {n}", "Un coup de boost pour la suite ?"),
        ("COUP DE MOU ?", "Une playlist pour relancer la machine ?"),
        ("{n}, L'APRÈM' EST LONGUE", "Qu'est-ce qu'on écoute ?"),
        ("L'APRÈS-MIDI BAT SON PLEIN", "Focus ou énergie ?"),
        ("SALUT {n}", "Un son pour t'accompagner jusqu'au soir ?"),
        ("ON GARDE LE RYTHME", "Quel titre pour la suite ?"),
    ],
    &[
        ("BONSOIR, {n}", "Qu'est-ce qu'on écoute ce soir ?"),
        ("BONNE SOIRÉE", "Quelle ambiance pour la soirée ?"),
        ("LA JOURNÉE EST FINIE, {n}", "On se détend en musique ?"),
        ("{n}, TU L'AS BIEN MÉRITÉ", "Un son pour décompresser ?"),
        ("C'EST LE SOIR", "Chill ou grosse soirée ?"),
        ("BONSOIR", "Quelle musique pour finir la journée ?"),
    ],
    &[
        ("ENCORE DEBOUT, {n} ?", "Un dernier morceau avant de dormir ?"),
        ("NUIT BLANCHE ?", "Qu'est-ce qu'on écoute à cette heure-ci ?"),
        ("{n}, LA NUIT EST À TOI", "Une playlist pour les oiseaux de nuit ?"),
        ("IL EST TARD", "Quelque chose de doux pour la nuit ?"),
        ("CHUT, LA VILLE DORT", "Quel son pour les petites heures ?"),
        ("NOCTAMBULE, {n} ?", "Un night drive en musique ?"),
    ],
];

fn greeting(lang: Language, hour: u32, name: Option<&str>, seed: usize) -> (String, String) {
    let n = name.unwrap_or("");
    let slot = day_slot(hour);
    let (label, title) = match lang {
        Language::En => match slot {
            0 => ("GOOD MORNING".to_string(), "A song to start the day?".to_string()),
            1 => ("GOOD MORNING".to_string(), "What are we listening to?".to_string()),
            2 => ("LUNCH TIME".to_string(), "Some music for your break?".to_string()),
            3 => (
                "GOOD AFTERNOON".to_string(),
                "A little boost for the rest of the day?".to_string(),
            ),
            4 => (
                "GOOD EVENING".to_string(),
                "What will you listen to tonight?".to_string(),
            ),
            _ => ("NIGHT OWL".to_string(), "A last track before sleep?".to_string()),
        },
        Language::Fr => {
            let pool: Vec<&(&str, &str)> = GREETINGS[slot]
                .iter()
                .filter(|(l, t)| name.is_some() || !(l.contains("{n}") || t.contains("{n}")))
                .collect();
            let (l, t) = pool[(seed.wrapping_add(slot * 7)) % pool.len()];
            (l.replace("{n}", n).to_uppercase(), t.replace("{n}", n))
        }
        Language::It => match slot {
            0 => (
                "BUON GIORNO".to_string(),
                "Un brano per iniziare la giornata?".to_string(),
            ),
            1 => ("BUONGIORNO".to_string(), "Cosa ascoltiamo?".to_string()),
            2 => ("PAUSA PRANZO".to_string(), "Un po' di musica per la pausa?".to_string()),
            3 => (
                "BUON POMERIGGIO".to_string(),
                "Un po' di energia per il resto del giorno?".to_string(),
            ),
            4 => ("BUONA SERA".to_string(), "Cosa ascolti stasera?".to_string()),
            _ => (
                "NOTTAMBULO".to_string(),
                "Un ultimo brano prima di dormire?".to_string(),
            ),
        },
        Language::Es => match slot {
            0 => (
                "BUENOS DÍAS".to_string(),
                "¿Una canción para empezar el día?".to_string(),
            ),
            1 => ("BUENOS DÍAS".to_string(), "¿Qué escuchamos?".to_string()),
            2 => (
                "HORA DE COMER".to_string(),
                "¿Un poco de música para la pausa?".to_string(),
            ),
            3 => (
                "BUENAS TARDES".to_string(),
                "¿Un impulso para lo que queda del día?".to_string(),
            ),
            4 => ("BUENAS NOCHES".to_string(), "¿Qué escucharás esta noche?".to_string()),
            _ => (
                "TRASMONTANO".to_string(),
                "¿Una última canción antes de dormir?".to_string(),
            ),
        },
        Language::Zh => match slot {
            0 => ("早上好".to_string(), "用什么歌开启今天？".to_string()),
            1 => ("上午好".to_string(), "现在想听什么？".to_string()),
            2 => ("午餐时间".to_string(), "午休听点音乐？".to_string()),
            3 => ("下午好".to_string(), "来点活力陪伴下午？".to_string()),
            4 => ("晚上好".to_string(), "今晚想听什么？".to_string()),
            _ => ("夜猫子".to_string(), "睡前再听一首？".to_string()),
        },
        Language::Ru => match slot {
            0 => ("ДОБРОЕ УТРО".to_string(), "Начнём день с музыки?".to_string()),
            1 => ("ДОБРОЕ УТРО".to_string(), "Что будем слушать?".to_string()),
            2 => ("ОБЕДЕННЫЙ ПЕРЕРЫВ".to_string(), "Музыка на время паузы?".to_string()),
            3 => (
                "ДОБРЫЙ ДЕНЬ".to_string(),
                "Добавим энергии на оставшуюся часть дня?".to_string(),
            ),
            4 => ("ДОБРЫЙ ВЕЧЕР".to_string(), "Что послушаем сегодня вечером?".to_string()),
            _ => ("ПОЛУНОЧНИК".to_string(), "Ещё один трек перед сном?".to_string()),
        },
    };
    (label.replace("{n}", n).to_string(), title.replace("{n}", n).to_string())
}

#[derive(Clone, Copy)]
struct MiniAnim {
    t0: f64,
    from_size: Vec2,
    to_size: Vec2,
    from_pos: Option<Pos2>,
    to_pos: Option<Pos2>,
}

#[derive(Clone, Copy)]
struct ExitAnim {
    t_start: f64,
    t0: Option<f64>,
    settle: Option<f64>,
    fade0: Option<f64>,
    from: Rect,
    to: Rect,
    uni: Rect,
}

#[derive(Clone, Copy)]
struct SwitchAnim {
    t0: f64,
    who: Option<usize>,
    swapped: bool,
}
const SW_OUT: f64 = 0.18;
const SW_IN: f64 = 0.34;
const HERO_H: f32 = 210.0;
const LY_H: f32 = 310.0;
const ANIM_FADE: f64 = 0.12;
const ANIM_MOVE: f64 = 0.30;
const MIN_W: f32 = 800.0;
const MIN_H: f32 = 600.0;

struct App {
    view: View,
    playing: bool,
    progress: f32,
    volume: f32,
    shuffle: bool,
    repeat: bool,
    query: String,
    mood: usize,
    tab: usize,
    caret: Caret,

    hits: Vec<Hit>,
    searching: bool,
    search_err: Option<String>,
    seen_query: String,
    sent_query: String,
    edit_t: f64,
    rx: Option<Receiver<SearchMsg>>,
    sel: Option<Hit>,
    nav_y: Option<f32>,
    recent: Vec<String>,
    history: Vec<Hit>,
    drop_open: bool,
    drop_rect: Option<Rect>,
    all_for_you: bool,
    hero_bg: Option<Color32>,

    started: bool,
    queue: Vec<Hit>,
    qi: usize,

    likes: Vec<Hit>,
    liked_ids: std::collections::HashSet<String>,
    playlists: Vec<Playlist>,
    open_pl: Option<usize>,
    pl_edit: Option<PlEdit>,
    pl_text: String,
    pl_focus: bool,
    pl_del: Option<usize>,
    pl_menu: Option<PlMenu>,
    pl_ctx: Option<(usize, Pos2)>,

    home_cache: HashMap<String, Arc<Vec<Hit>>>,
    home_err: HashMap<String, String>,
    home_pending: std::collections::HashSet<String>,
    home_tx: Sender<SearchMsg>,
    home_rx: Receiver<SearchMsg>,
    fy_seed: String,
    imgs: ImgCache,
    audio: Audio,
    auto_retry: Option<String>,
    seeking: bool,
    pf_for: String,
    shuf_next: Option<usize>,
    last_sync: f64,
    last_playing: bool,
    mini: bool,
    restore: Option<(Vec2, Option<Pos2>)>,
    mini_anim: Option<MiniAnim>,
    exit_anim: Option<ExitAnim>,
    fade_a: f32,
    card_drag: bool,
    toast: Option<(String, f64)>,

    profiles: Vec<Profile>,
    cur: Option<usize>,
    language: Language,
    lang_open: bool,
    pm_del: bool,
    pui: Pui,
    entered: bool,
    form: Option<Form>,
    photos: HashMap<String, Option<TextureHandle>>,
    fy_taste: Option<usize>,
    greet_seed: usize,
    sw: Option<SwitchAnim>,
    glass_tries: u32,

    acc_pick: bool,
    acc_hsv: [f32; 3],
    acc_dirty: bool,
    acc_applied: u32,
    ly_open: bool,
    ly: HashMap<String, LySlot>,
    ly_tx: Sender<(String, LyRes)>,
    ly_rx: Receiver<(String, LyRes)>,
    ly_for: String,
    ly_y: f32,
    ly_track: String,
    ly_h: (String, u32, Vec<f32>),

    dc: Sender<discord::Msg>,
    dc_on: bool,
    dc_sent: Option<(String, i64)>,
    dc_cand: Option<(Option<(String, i64)>, f64)>,

    dep: Dep,
    dep_want: (bool, bool),
    dep_rx: Option<Receiver<(bool, bool)>>,
    dep_prog: Arc<Mutex<DepProgress>>,
}

impl App {
    fn new(ctx: &Context) -> Self {
        quiet_rodio_panics();
        cache_init();
        wipe_guest();
        let likes = load_likes();
        let liked_ids = likes.iter().map(|h| h.id.clone()).collect();
        let history = load_history();
        let fy_seed = history
            .first()
            .map(|h| h.artist.clone())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| "new music hits 2026".to_string());
        let (home_tx, home_rx) = channel();
        let (ly_tx, ly_rx) = channel();

        let (dep_tx, dep_rx) = channel();
        {
            let c = ctx.clone();
            std::thread::spawn(move || {
                let _ = dep_tx.send(missing_tools());
                c.request_repaint();
            });
        }
        Self {
            view: View::Home,
            playing: false,
            progress: 0.0,
            volume: 0.7,
            shuffle: false,
            repeat: false,
            query: String::new(),
            mood: 0,
            tab: 0,
            caret: Caret::default(),
            hits: Vec::new(),
            searching: false,
            search_err: None,
            seen_query: String::new(),
            sent_query: String::new(),
            edit_t: 0.0,
            rx: None,
            sel: None,
            nav_y: None,
            recent: load_recent(),
            history,
            drop_open: false,
            drop_rect: None,
            all_for_you: false,
            hero_bg: None,
            started: false,
            queue: Vec::new(),
            qi: 0,
            likes,
            liked_ids,
            playlists: load_playlists(),
            open_pl: None,
            pl_edit: None,
            pl_text: String::new(),
            pl_focus: false,
            pl_del: None,
            pl_menu: None,
            pl_ctx: None,
            home_cache: HashMap::new(),
            home_err: HashMap::new(),
            home_pending: Default::default(),
            home_tx,
            home_rx,
            fy_seed,
            imgs: ImgCache::new(ctx),
            audio: Audio::new(ctx.clone()),
            seeking: false,
            mini: false,
            restore: None,
            mini_anim: None,
            exit_anim: None,
            fade_a: 0.0,
            card_drag: false,
            toast: None,
            profiles: load_profiles(),
            cur: None,
            language: load_language(),
            lang_open: false,
            pm_del: false,
            pui: Pui::Picker,
            entered: false,
            form: None,
            photos: HashMap::new(),
            fy_taste: None,
            greet_seed: rand_below(10_000),
            sw: None,
            glass_tries: 0,
            acc_pick: false,
            acc_hsv: [0.0; 3],
            acc_dirty: false,
            acc_applied: ACCENT_DEFAULT,
            ly_open: false,
            ly: HashMap::new(),
            ly_tx,
            ly_rx,
            ly_for: String::new(),
            ly_y: 0.0,
            ly_track: String::new(),
            ly_h: (String::new(), 0, Vec::new()),
            auto_retry: None,
            last_playing: false,
            pf_for: String::new(),
            shuf_next: None,
            last_sync: -2.0,
            dc: discord::spawn(),
            dc_on: load_discord_pref(),
            dc_sent: None,
            dc_cand: None,
            dep: Dep::Idle,
            dep_want: (false, false),
            dep_rx: Some(dep_rx),
            dep_prog: Arc::new(Mutex::new(DepProgress {
                label: String::new(),
                frac: 0.0,
                result: None,
            })),
        }
    }
}

impl App {
    fn dep_tick(&mut self, ctx: &Context) {
        let got = self.dep_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some((y, f)) = got {
            self.dep_rx = None;
            if y || f {
                self.dep = Dep::Ask { ytdlp: y, ffmpeg: f };
            }
        }
        if self.dep == Dep::Installing {
            let res = self.dep_prog.lock().unwrap_or_else(|e| e.into_inner()).result.take();
            match res {
                Some(Ok(())) => self.dep = Dep::Done,
                Some(Err(e)) => self.dep = Dep::Failed(e),
                None => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
    }

    fn dep_start(&mut self, ctx: &Context, ytdlp: bool, ffmpeg: bool) {
        self.dep_want = (ytdlp, ffmpeg);
        *self.dep_prog.lock().unwrap_or_else(|e| e.into_inner()) = DepProgress {
            label: tr(self.language, "preparing").to_string(),
            frac: 0.0,
            result: None,
        };
        self.dep = Dep::Installing;
        let (st, c, lang) = (self.dep_prog.clone(), ctx.clone(), self.language);
        std::thread::spawn(move || {
            let r = install_tools(ytdlp, ffmpeg, &st, &c, lang);
            st.lock().unwrap_or_else(|e| e.into_inner()).result = Some(r);
            c.request_repaint();
        });
    }

    fn solid_btn(ui: &mut Ui, r: Rect, label: &str, id: &str) -> bool {
        let rs = ui.interact(r, Id::new(id), Sense::click());
        let rz = pop(ui, &rs, r, 2.0);
        ui.painter().rect_filled(rz, 8.0, accent());
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(14.0),
            on_accent(),
        );
        hand(ui, &rs);
        rs.clicked()
    }

    fn dep_dialog(&mut self, ui: &mut Ui, full: Rect) {
        if self.dep == Dep::Idle {
            return;
        }
        let ctx = ui.ctx().clone();
        let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
        let rad = if maximized { 0.0 } else { WIN_R };
        ui.interact(full, Id::new("dep_block"), Sense::click_and_drag());
        let p = ui.painter().clone();
        let veil = ctx.animate_bool_with_time(Id::new("dep_veil"), true, 0.2);
        p.rect_filled(
            full,
            Rounding {
                nw: 0.0,
                ne: 0.0,
                sw: rad,
                se: rad,
            },
            Color32::from_black_alpha((150.0 * veil) as u8),
        );

        let dep = self.dep.clone();
        let w = 460.0_f32.min(full.width() - 32.0);
        let (pad, rd) = (28.0_f32, 14.0_f32);
        let (x, iw) = (0.0_f32, w - pad * 2.0);
        let f13 = FontId::proportional(13.0);
        let f12 = FontId::proportional(12.0);

        let mut g1 = None;
        let mut g2 = None;
        let mut n_rows = 0usize;
        let head = 26.0 + 16.0 + 8.0 + 32.0;
        let btns = 24.0 + 40.0 + pad - 4.0;
        let h = match &dep {
            Dep::Idle => 0.0,
            Dep::Ask { ytdlp, ffmpeg } => {
                n_rows = *ytdlp as usize + *ffmpeg as usize;
                let a = layout_words(&p, match self.language {
                    Language::En => "Rustify needs yt-dlp (download) and ffmpeg (audio conversion) to play music.",
                    Language::Fr => "Rustify a besoin de yt-dlp (téléchargement) et de ffmpeg (conversion audio) pour lire la musique.",
                    Language::It => "Rustify richiede yt-dlp (download) e ffmpeg (conversione audio) per riprodurre la musica.",
                    Language::Es => "Rustify necesita yt-dlp (descarga) y ffmpeg (conversión de audio) para reproducir música.",
                    Language::Zh => "Rustify 需要 yt-dlp（下载）和 ffmpeg（音频转换）才能播放音乐。",
                    Language::Ru => "Для воспроизведения музыки Rustify нужны yt-dlp (загрузка) и ffmpeg (преобразование аудио).",
                }, f13.clone(), MUTED, iw, 4);
                let b = layout_words(
                    &p,
                    match self.language {
                        Language::En => "Direct download into Rustify's data folder, without administrator rights.",
                        Language::Fr => {
                            "Téléchargement direct dans le dossier de données de Rustify, sans droits administrateur."
                        }
                        Language::It => {
                            "Download diretto nella cartella dati di Rustify, senza diritti amministratore."
                        }
                        Language::Es => {
                            "Descarga directa en la carpeta de datos de Rustify, sin permisos de administrador."
                        }
                        Language::Zh => "直接下载到 Rustify 数据目录，无需管理员权限。",
                        Language::Ru => "Прямая загрузка в папку данных Rustify без прав администратора.",
                    },
                    f12.clone(),
                    MUTED,
                    iw,
                    4,
                );
                let hh = head + a.size().y + 16.0 + n_rows as f32 * 48.0 + 4.0 + b.size().y + btns;
                g1 = Some(a);
                g2 = Some(b);
                hh
            }
            Dep::Installing => {
                let a = layout_words(
                    &p,
                    match self.language {
                        Language::En => "Do not close Rustify during installation.",
                        Language::Fr => "Ne ferme pas Rustify pendant l'installation.",
                        Language::It => "Non chiudere Rustify durante l'installazione.",
                        Language::Es => "No cierres Rustify durante la instalación.",
                        Language::Zh => "安装期间请勿关闭 Rustify。",
                        Language::Ru => "Не закрывайте Rustify во время установки.",
                    },
                    f12.clone(),
                    MUTED,
                    iw,
                    2,
                );
                let hh = head + 22.0 + 14.0 + 6.0 + 16.0 + a.size().y + pad;
                g1 = Some(a);
                hh
            }
            Dep::Done => {
                let a = layout_words(
                    &p,
                    match self.language {
                        Language::En => "yt-dlp and ffmpeg are ready. You can start playback.",
                        Language::Fr => "yt-dlp et ffmpeg sont prêts. Tu peux lancer la lecture.",
                        Language::It => "yt-dlp e ffmpeg sono pronti. Puoi avviare la riproduzione.",
                        Language::Es => "yt-dlp y ffmpeg están listos. Puedes iniciar la reproducción.",
                        Language::Zh => "yt-dlp 和 ffmpeg 已准备就绪。现在可以开始播放。",
                        Language::Ru => "yt-dlp и ffmpeg готовы. Можно начинать воспроизведение.",
                    },
                    f13.clone(),
                    MUTED,
                    iw,
                    3,
                );
                let hh = head + a.size().y + btns;
                g1 = Some(a);
                hh
            }
            Dep::Failed(msg) => {
                let a = layout_words(&p, msg, f13.clone(), MUTED, iw, 6);
                let hh = head + a.size().y + btns;
                g1 = Some(a);
                hh
            }
        };

        let r = Rect::from_center_size(full.center(), vec2(w, h));
        ui.interact(r, Id::new("dep_panel_block"), Sense::click());

        for i in 1..=6 {
            let e = i as f32 * 3.0;
            p.rect_filled(
                r.expand(e).translate(vec2(0.0, 8.0)),
                rd + e,
                Color32::from_black_alpha(9),
            );
        }
        acrylic_panel(&p, r, rd);
        p.rect_stroke(r.shrink(0.5), rd, Stroke::new(1.0_f32, LINE));

        let x = r.left() + pad + x;
        let mut y = r.top() + 26.0;
        let esc = ctx.input(|i| i.key_pressed(Key::Escape));
        let (by, bw) = (r.bottom() - pad - 40.0 + 4.0, 118.0_f32);
        let b_ok = Rect::from_min_size(pos2(r.right() - pad - bw, by), vec2(bw, 40.0));
        let b_no = Rect::from_min_size(pos2(b_ok.left() - 8.0 - bw, by), vec2(bw, 40.0));

        let header = |p: &Painter, y: &mut f32, label: &str, title: &str| {
            spaced(p, pos2(x, *y + 6.0), Align2::LEFT_CENTER, label, 11.0, accent());
            *y += 16.0 + 8.0;
            p.text(pos2(x, *y), Align2::LEFT_TOP, title, FontId::proportional(22.0), TEXT);
            *y += 32.0;
        };

        match dep {
            Dep::Idle => {}
            Dep::Ask { ytdlp, ffmpeg } => {
                header(
                    &p,
                    &mut y,
                    tr(self.language, "configuration"),
                    tr(self.language, "components_missing"),
                );
                if let Some(g) = g1 {
                    let gh = g.size().y;
                    p.galley(pos2(x, y), g, MUTED);
                    y += gh + 16.0;
                }
                for (missing, name) in [(ytdlp, "yt-dlp"), (ffmpeg, "ffmpeg")] {
                    if !missing {
                        continue;
                    }
                    let row = Rect::from_min_size(pos2(x, y), vec2(iw, 40.0));
                    p.rect_filled(row, 8.0, SURF2);
                    p.rect_stroke(row.shrink(0.5), 8.0, Stroke::new(1.0_f32, LINE));
                    p.circle_filled(pos2(row.left() + 16.0, row.center().y), 3.0, accent());
                    p.text(
                        pos2(row.left() + 30.0, row.center().y),
                        Align2::LEFT_CENTER,
                        name,
                        FontId::proportional(14.0),
                        TEXT,
                    );
                    p.text(
                        pos2(row.right() - 14.0, row.center().y),
                        Align2::RIGHT_CENTER,
                        tr(self.language, "not_found"),
                        f12.clone(),
                        MUTED,
                    );
                    y += 48.0;
                }
                y += 4.0;
                if let Some(g) = g2 {
                    p.galley(pos2(x, y), g, MUTED);
                }
                let _ = n_rows;
                if Self::solid_btn(ui, b_ok, tr(self.language, "install"), "dep_ok") {
                    self.dep_start(&ctx, ytdlp, ffmpeg);
                } else if Self::text_btn(ui, b_no, tr(self.language, "later"), "dep_no", false) || esc {
                    self.dep = Dep::Idle;
                }
            }
            Dep::Installing => {
                header(
                    &p,
                    &mut y,
                    tr(self.language, "installation"),
                    tr(self.language, "installing"),
                );
                let (label, frac) = {
                    let g = self.dep_prog.lock().unwrap_or_else(|e| e.into_inner());
                    (g.label.clone(), g.frac)
                };
                let shown = ctx
                    .animate_value_with_time(Id::new("dep_bar"), frac, 0.25)
                    .clamp(0.0, 1.0);
                p.text(pos2(x, y), Align2::LEFT_TOP, &label, f13.clone(), TEXT);
                p.text(
                    pos2(r.right() - pad, y),
                    Align2::RIGHT_TOP,
                    format!("{} %", (shown * 100.0).round() as u32),
                    f13.clone(),
                    MUTED,
                );
                y += 22.0 + 14.0;
                let bar = Rect::from_min_size(pos2(x, y), vec2(iw, 6.0));
                p.rect_filled(bar, 3.0, SURF2);
                if shown > 0.005 {
                    p.rect_filled(
                        Rect::from_min_size(bar.min, vec2((iw * shown).max(6.0), 6.0)),
                        3.0,
                        accent(),
                    );
                }
                y += 6.0 + 16.0;
                if let Some(g) = g1 {
                    p.galley(pos2(x, y), g, MUTED);
                }
            }
            Dep::Done => {
                header(&p, &mut y, tr(self.language, "done"), tr(self.language, "installed"));
                if let Some(g) = g1 {
                    p.galley(pos2(x, y), g, MUTED);
                }
                if Self::solid_btn(ui, b_ok, tr(self.language, "finish"), "dep_ok") || esc {
                    self.dep = Dep::Idle;
                }
            }
            Dep::Failed(_) => {
                header(
                    &p,
                    &mut y,
                    tr(self.language, "error"),
                    tr(self.language, "install_failed"),
                );
                if let Some(g) = g1 {
                    p.galley(pos2(x, y), g, MUTED);
                }
                if Self::solid_btn(ui, b_ok, tr(self.language, "retry"), "dep_ok") {
                    let (a, b) = self.dep_want;
                    self.dep_start(&ctx, a, b);
                } else if Self::text_btn(ui, b_no, tr(self.language, "close"), "dep_no", false) || esc {
                    self.dep = Dep::Idle;
                }
            }
        }
    }
}

impl App {
    fn discord_sync(&mut self, ctx: &Context) {
        if self.audio.loading {
            return;
        }
        type St = Option<(String, i64)>;
        fn same(a: &St, b: &St) -> bool {
            match (a, b) {
                (None, None) => true,
                (Some((x, s)), Some((y, t))) => x == y && (s - t).abs() <= 2,
                _ => false,
            }
        }
        let now = ctx.input(|i| i.time);
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let want: St = match self.now_id() {
            Some(id) if self.dc_on && self.playing && self.audio.pos_since.is_some() && self.audio.err.is_none() => {
                Some((id, unix - self.audio.pos() as i64))
            }
            _ => None,
        };
        if same(&want, &self.dc_sent) {
            self.dc_cand = None;
            return;
        }
        let prev = self.dc_cand.as_ref().filter(|(c, _)| same(c, &want)).map(|(_, t)| *t);
        let since = match prev {
            Some(t) => t,
            None => {
                self.dc_cand = Some((want.clone(), now));
                now
            }
        };
        if now - since < 1.0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        let msg = match &want {
            None => discord::Msg::Clear,
            Some((id, start)) => {
                let h = self.now();
                let dur = self
                    .audio
                    .dur
                    .filter(|d| *d > 0.0)
                    .or(if h.secs > 0 { Some(h.secs as f32) } else { None });

                let mut image = h.thumb.replace("=w96-h96", "=w512-h512");
                if !image.starts_with("http") {
                    image.clear();
                }
                discord::Msg::Set(discord::Presence {
                    details: h.title.clone(),
                    state: if h.artist.is_empty() {
                        "Rustify".to_string()
                    } else {
                        h.artist.clone()
                    },
                    image,
                    image_text: if h.album.is_empty() {
                        h.title.clone()
                    } else {
                        h.album.clone()
                    },
                    url: format!("https://music.youtube.com/watch?v={id}"),
                    start: *start,
                    end: dur.map(|d| *start + d as i64),
                })
            }
        };
        let _ = self.dc.send(msg);
        self.dc_sent = want;
        self.dc_cand = None;
    }
}

#[cfg(windows)]
mod win_glass {
    use std::ffi::c_void;
    type Hwnd = *mut c_void;

    #[repr(C)]
    struct Margins {
        l: i32,
        r: i32,
        t: i32,
        b: i32,
    }
    struct Find {
        pid: u32,
        found: Hwnd,
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(hwnd: Hwnd, attr: u32, val: *const c_void, size: u32) -> i32;
        fn DwmExtendFrameIntoClientArea(hwnd: Hwnd, m: *const Margins) -> i32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(cb: extern "system" fn(Hwnd, isize) -> i32, lp: isize) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn GetWindowTextW(hwnd: Hwnd, buf: *mut u16, max: i32) -> i32;
        fn GetWindowLongW(hwnd: Hwnd, idx: i32) -> i32;
        fn SetWindowLongW(hwnd: Hwnd, idx: i32, v: i32) -> i32;
        fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
    }

    extern "system" fn on_window(h: Hwnd, lp: isize) -> i32 {
        let f = unsafe { &mut *(lp as *mut Find) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(h, &mut pid) };
        if pid == f.pid {
            let mut buf = [0u16; 64];
            let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), 64) };
            if n > 0 && String::from_utf16_lossy(&buf[..n as usize]) == "Rustify" {
                f.found = h;
                return 0;
            }
        }
        1
    }

    static HWND_CACHE: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

    pub fn strip_sysmenu() {
        let h = HWND_CACHE.load(std::sync::atomic::Ordering::Relaxed) as Hwnd;
        if h.is_null() {
            return;
        }
        const GWL_STYLE: i32 = -16;
        const WS_SYSMENU: i32 = 0x0008_0000;
        let st = unsafe { GetWindowLongW(h, GWL_STYLE) };
        if st & WS_SYSMENU != 0 {
            unsafe {
                SetWindowLongW(h, GWL_STYLE, st & !WS_SYSMENU);

                SetWindowPos(
                    h,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    0x0001 | 0x0002 | 0x0004 | 0x0010 | 0x0020,
                );
            }
        }
    }

    fn set(h: Hwnd, attr: u32, v: i32) -> i32 {
        unsafe { DwmSetWindowAttribute(h, attr, &v as *const i32 as *const c_void, 4) }
    }

    pub fn apply() -> Option<bool> {
        let mut f = Find {
            pid: unsafe { GetCurrentProcessId() },
            found: std::ptr::null_mut(),
        };
        unsafe { EnumWindows(on_window, &mut f as *mut Find as isize) };
        if f.found.is_null() {
            return None;
        }
        let h = f.found;
        HWND_CACHE.store(h as isize, std::sync::atomic::Ordering::Relaxed);
        strip_sysmenu();
        set(h, 3, 1);
        set(h, 20, 1);
        set(h, 33, 2);
        set(h, 34, 0xFFFF_FFFEu32 as i32);
        let m = Margins {
            l: -1,
            r: -1,
            t: -1,
            b: -1,
        };
        unsafe { DwmExtendFrameIntoClientArea(h, &m) };
        Some(set(h, 38, 3) == 0)
    }
}

#[cfg(not(windows))]
mod win_glass {
    pub fn apply() -> Option<bool> {
        Some(false)
    }
    pub fn strip_sysmenu() {}
}

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 840.0])
            .with_min_inner_size([MIN_W, MIN_H])
            .with_decorations(false)
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native(
        "Rustify",
        options,
        Box::new(|cc| {
            let mut v = Visuals::dark();
            v.selection.bg_fill = accent();
            v.text_cursor.stroke = Stroke::NONE;
            cc.egui_ctx.set_visuals(v);
            setup_fonts(&cc.egui_ctx);
            cc.egui_ctx.style_mut(|s| s.spacing.item_spacing = vec2(0.0, 0.0));
            Ok(Box::new(App::new(&cc.egui_ctx)))
        }),
    )
}

fn cover(p: &Painter, r: Rect, seed: usize, rd: f32, inv: bool) {
    let pal = PAL[seed % PAL.len()];
    let (bg, fg) = if inv { (pal[1], pal[0]) } else { (pal[0], pal[1]) };
    p.rect_filled(r, rd, bg);
    let s = r.width();
    let at = |x: f32, y: f32| r.min + vec2(x, y) * s;
    let cl = p.with_clip_rect(r);
    match (seed + seed / 4) % 4 {
        0 => {
            cl.circle_filled(at(0.5, 0.5), 0.30 * s, fg);
            cl.circle_filled(at(0.80, 0.22), 0.06 * s, fg);
        }
        1 => {
            for (k, wd) in [0.70f32, 0.50, 0.62, 0.36].iter().enumerate() {
                let y = 0.20 + k as f32 * 0.17;
                cl.rect_filled(Rect::from_min_size(at(0.15, y), vec2(wd * s, 0.08 * s)), 0.04 * s, fg);
            }
        }
        2 => {
            cl.circle_filled(at(0.5, 1.0), 0.40 * s, fg);
            cl.circle_filled(at(0.5, 0.28), 0.08 * s, fg);
        }
        _ => {
            for rr in [0.12f32, 0.22, 0.32] {
                cl.circle_stroke(at(0.5, 0.5), rr * s, Stroke::new((s * 0.035).max(2.0), fg));
            }
        }
    }
}

fn genre_icon(p: &Painter, r: Rect, idx: usize, fg: Color32, bg: Color32) {
    use std::f32::consts::PI;
    let s = r.width();
    let at = |x: f32, y: f32| r.min + vec2(x, y) * s;
    let st = Stroke::new(0.075 * s, fg);

    let fan = |c: Pos2, pts: Vec<Pos2>| {
        let mut m = Mesh::default();
        m.colored_vertex(c, fg);
        for q in &pts {
            m.colored_vertex(*q, fg);
        }
        let n = pts.len() as u32;
        for i in 0..n {
            m.add_triangle(0, i + 1, 1 + (i + 1) % n);
        }
        p.add(Shape::mesh(m));
    };

    let star = |c: (f32, f32), n: usize, ro: f32, ri: f32| -> Vec<Pos2> {
        (0..n * 2)
            .map(|k| {
                let a = -PI / 2.0 + k as f32 * TAU / (n * 2) as f32;
                let rr = if k % 2 == 0 { ro } else { ri };
                at(c.0 + rr * a.cos(), c.1 + rr * a.sin())
            })
            .collect()
    };
    match idx {
        0 => fan(at(0.5, 0.52), star((0.5, 0.52), 5, 0.44, 0.19)),

        1 => {
            p.rect_filled(Rect::from_min_max(at(0.36, 0.08), at(0.64, 0.5)), 0.14 * s, fg);
            let u: Vec<Pos2> = (0..=12)
                .map(|i| {
                    let a = PI * i as f32 / 12.0;
                    at(0.5 + 0.26 * a.cos(), 0.42 + 0.26 * a.sin())
                })
                .collect();
            p.add(Shape::line(u, st));
            p.line_segment([at(0.5, 0.68), at(0.5, 0.9)], st);
            p.line_segment([at(0.36, 0.9), at(0.64, 0.9)], st);
        }

        2 => {
            p.add(Shape::convex_polygon(
                vec![at(0.58, 0.05), at(0.25, 0.55), at(0.47, 0.55), at(0.53, 0.42)],
                fg,
                Stroke::NONE,
            ));
            p.add(Shape::convex_polygon(
                vec![at(0.47, 0.55), at(0.40, 0.95), at(0.75, 0.42), at(0.53, 0.42)],
                fg,
                Stroke::NONE,
            ));
        }

        3 => {
            let c = (0.5f32, 0.42f32);
            let mut pts: Vec<Pos2> = Vec::new();
            for ang in [90.0f32, 210.0, 330.0] {
                let a = ang.to_radians();
                let (vx, vy) = (c.0 + 0.26 * a.cos(), c.1 + 0.26 * a.sin());
                for j in 0..=10 {
                    let b = a + (-60.0 + 120.0 * j as f32 / 10.0).to_radians();
                    pts.push(at(vx + 0.15 * b.cos(), vy + 0.15 * b.sin()));
                }
            }
            p.add(Shape::convex_polygon(pts, fg, Stroke::NONE));
        }

        4 => {
            p.rect_filled(Rect::from_min_max(at(0.08, 0.22), at(0.92, 0.82)), 0.06 * s, fg);
            for k in 0..4 {
                let x0 = 0.12 + k as f32 * 0.195;
                p.rect_filled(Rect::from_min_max(at(x0, 0.26), at(x0 + 0.17, 0.78)), 0.03 * s, bg);
            }
            for k in 1..4 {
                let xc = 0.12 + k as f32 * 0.195 - 0.0125;
                p.rect_filled(
                    Rect::from_min_max(at(xc - 0.05, 0.26), at(xc + 0.05, 0.55)),
                    0.02 * s,
                    fg,
                );
            }
        }

        5 => {
            p.circle_filled(at(0.46, 0.5), 0.34 * s, fg);
            p.circle_filled(at(0.62, 0.4), 0.30 * s, bg);
            p.circle_filled(at(0.85, 0.78), 0.04 * s, fg);
            p.circle_filled(at(0.18, 0.16), 0.035 * s, fg);
        }

        6 => {
            p.circle_filled(at(0.5, 0.7), 0.22 * s, fg);
            p.circle_filled(at(0.5, 0.45), 0.16 * s, fg);
            p.rect_filled(Rect::from_min_max(at(0.47, 0.06), at(0.53, 0.38)), 0.02 * s, fg);
            p.circle_filled(at(0.5, 0.07), 0.045 * s, fg);
            let f = Stroke::new(0.03 * s, bg);
            p.line_segment([at(0.38, 0.6), at(0.38, 0.78)], f);
            p.line_segment([at(0.62, 0.6), at(0.62, 0.78)], f);
        }

        7 => {
            p.rect_filled(Rect::from_min_max(at(0.06, 0.2), at(0.94, 0.8)), 0.08 * s, fg);
            p.circle_filled(at(0.32, 0.45), 0.11 * s, bg);
            p.circle_filled(at(0.68, 0.45), 0.11 * s, bg);
            p.rect_filled(Rect::from_min_max(at(0.26, 0.64), at(0.74, 0.74)), 0.04 * s, bg);
        }

        8 => icon(p, Ic::Heart(true), at(0.5, 0.5), 0.8 * s, fg),

        9 => {
            p.circle_filled(at(0.5, 0.5), 0.2 * s, fg);
            for k in 0..8 {
                let a = k as f32 * TAU / 8.0;
                p.line_segment(
                    [
                        at(0.5 + 0.30 * a.cos(), 0.5 + 0.30 * a.sin()),
                        at(0.5 + 0.44 * a.cos(), 0.5 + 0.44 * a.sin()),
                    ],
                    st,
                );
            }
        }

        10 => {
            fan(at(0.5, 0.5), star((0.5, 0.5), 8, 0.46, 0.26));
            p.circle_filled(at(0.5, 0.5), 0.1 * s, bg);
        }

        11 => {
            fan(at(0.45, 0.55), star((0.45, 0.55), 4, 0.40, 0.09));
            fan(at(0.78, 0.22), star((0.78, 0.22), 4, 0.17, 0.04));
            fan(at(0.78, 0.84), star((0.78, 0.84), 4, 0.12, 0.03));
        }
        _ => {}
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum Ic {
    Home(bool),
    Search,
    Library,
    Play,
    Pause,
    Next,
    Prev,
    Shuffle,
    Repeat,
    Heart(bool),
    Vol,
    Plus,
    Queue,
    Share,
    Settings,
    Clock,
    Check,
    More,
    Lyrics,
}

fn icon(p: &Painter, ic: Ic, ctr: Pos2, s: f32, col: Color32) {
    let h = s / 2.0;
    let st = Stroke::new(1.8_f32, col);
    let v = |x: f32, y: f32| vec2(x * h, y * h);
    let ln = |a: Vec2, b: Vec2| {
        p.line_segment([ctr + a, ctr + b], st);
    };
    let tri = |a: Vec2, b: Vec2, d: Vec2| {
        p.add(Shape::convex_polygon(
            vec![ctr + a, ctr + b, ctr + d],
            col,
            Stroke::NONE,
        ));
    };
    match ic {
        Ic::Play => tri(v(-0.6, -1.0), v(-0.6, 1.0), v(1.0, 0.0)),
        Ic::Pause => {
            for x in [-0.55f32, 0.55] {
                p.rect_filled(Rect::from_center_size(ctr + v(x, 0.0), v(0.55, 2.0)), 1.5, col);
            }
        }
        Ic::Next => {
            tri(v(-1.0, -0.9), v(-1.0, 0.9), v(0.6, 0.0));
            p.rect_filled(Rect::from_center_size(ctr + v(0.9, 0.0), v(0.3, 1.8)), 1.0, col);
        }
        Ic::Prev => {
            tri(v(1.0, -0.9), v(1.0, 0.9), v(-0.6, 0.0));
            p.rect_filled(Rect::from_center_size(ctr + v(-0.9, 0.0), v(0.3, 1.8)), 1.0, col);
        }
        Ic::Shuffle => {
            p.add(Shape::line(
                vec![ctr + v(0.4, -0.9), ctr + v(0.9, -0.9), ctr + v(0.9, -0.4)],
                st,
            ));
            ln(v(-0.8, 0.8), v(0.9, -0.9));
            p.add(Shape::line(
                vec![ctr + v(0.4, 0.9), ctr + v(0.9, 0.9), ctr + v(0.9, 0.4)],
                st,
            ));
            ln(v(0.3, 0.3), v(0.9, 0.9));
            ln(v(-0.8, -0.8), v(-0.3, -0.3));
        }
        Ic::Repeat => {
            let arc = |c: (f32, f32), r: f32, a0: f32, a1: f32| -> Vec<Pos2> {
                (0..=8)
                    .map(|i| {
                        let a = a0 + (a1 - a0) * i as f32 / 8.0;
                        ctr + v(c.0 + r * a.cos(), c.1 + r * a.sin())
                    })
                    .collect()
            };
            let mut top = vec![ctr + v(-0.9, -0.1)];
            top.extend(arc((-0.5, -0.3), 0.4, std::f32::consts::PI, 1.5 * std::f32::consts::PI));
            top.push(ctr + v(0.9, -0.7));
            p.add(Shape::line(top, st));
            p.add(Shape::line(
                vec![ctr + v(0.5, -1.1), ctr + v(0.9, -0.7), ctr + v(0.5, -0.3)],
                st,
            ));
            let mut bot = vec![ctr + v(0.9, 0.1)];
            bot.extend(arc((0.5, 0.3), 0.4, 0.0, 0.5 * std::f32::consts::PI));
            bot.push(ctr + v(-0.9, 0.7));
            p.add(Shape::line(bot, st));
            p.add(Shape::line(
                vec![ctr + v(-0.5, 1.1), ctr + v(-0.9, 0.7), ctr + v(-0.5, 0.3)],
                st,
            ));
        }
        Ic::Heart(fill) => {
            const N: u32 = 64;
            let pts: Vec<Pos2> = (0..N)
                .map(|i| {
                    let t = i as f32 / N as f32 * TAU;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    ctr + vec2(x, -y + 1.0) * (h / 16.0)
                })
                .collect();
            if fill {
                let mut m = Mesh::default();
                m.colored_vertex(ctr, col);
                for q in &pts {
                    m.colored_vertex(*q, col);
                }
                for i in 0..N {
                    m.add_triangle(0, i + 1, 1 + (i + 1) % N);
                }
                p.add(Shape::mesh(m));
            }

            let mut outline = pts.clone();
            outline.push(pts[0]);
            p.add(Shape::line(outline, st));
            p.circle_filled(pts[0], st.width / 2.0, col);
        }
        Ic::Home(fill) => {
            let pts = vec![
                ctr + v(-1.0, -0.1),
                ctr + v(0.0, -1.0),
                ctr + v(1.0, -0.1),
                ctr + v(0.75, 1.0),
                ctr + v(-0.75, 1.0),
            ];
            if fill {
                p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
            } else {
                p.add(Shape::closed_line(pts, st));
            }
        }
        Ic::Search => {
            p.circle_stroke(ctr + v(-0.2, -0.2), h * 0.65, st);
            ln(v(0.3, 0.3), v(0.95, 0.95));
        }

        Ic::Settings => {
            for (y, kx) in [(-0.7f32, 0.35f32), (0.0, -0.4), (0.7, 0.2)] {
                ln(v(-1.0, y), v(kx - 0.36, y));
                ln(v(kx + 0.36, y), v(1.0, y));
                p.circle_stroke(ctr + v(kx, y), h * 0.26, st);
            }
        }

        Ic::More => {
            for y in [-0.7f32, 0.0, 0.7] {
                p.circle_filled(ctr + v(0.0, y), h * 0.17, col);
            }
        }

        Ic::Check => {
            p.add(Shape::line(
                vec![ctr + v(-0.85, 0.05), ctr + v(-0.25, 0.65), ctr + v(0.85, -0.6)],
                st,
            ));
        }

        Ic::Lyrics => {
            p.rect_stroke(Rect::from_center_size(ctr + v(0.0, -0.15), v(2.0, 1.6)), 3.0, st);
            p.add(Shape::line(
                vec![ctr + v(-0.55, 0.65), ctr + v(-0.75, 1.1), ctr + v(-0.1, 0.65)],
                st,
            ));
            ln(v(-0.55, -0.4), v(0.55, -0.4));
            ln(v(-0.55, 0.1), v(0.2, 0.1));
        }

        Ic::Clock => {
            p.circle_stroke(ctr, h * 0.95, st);
            ln(v(0.0, -0.5), v(0.0, 0.0));
            ln(v(0.0, 0.0), v(0.4, 0.3));
        }

        Ic::Plus => {
            ln(v(-1.0, -0.7), v(0.5, -0.7));
            ln(v(-1.0, 0.0), v(0.5, 0.0));
            ln(v(-1.0, 0.7), v(-0.1, 0.7));
            ln(v(0.25, 0.7), v(1.0, 0.7));
            ln(v(0.625, 0.325), v(0.625, 1.075));
        }

        Ic::Queue => {
            tri(v(-1.0, -0.65), v(-1.0, 0.65), v(-0.3, 0.0));
            ln(v(0.1, -0.7), v(1.0, -0.7));
            ln(v(0.1, 0.0), v(1.0, 0.0));
            ln(v(0.1, 0.7), v(1.0, 0.7));
        }

        Ic::Share => {
            ln(v(-0.8, 0.0), v(0.8, -0.75));
            ln(v(-0.8, 0.0), v(0.8, 0.75));
            for (x, y) in [(-0.8f32, 0.0f32), (0.8, -0.75), (0.8, 0.75)] {
                p.circle_filled(ctr + v(x, y), h * 0.26, col);
            }
        }
        Ic::Library => {
            for x in [-0.9f32, -0.35] {
                p.rect_filled(Rect::from_center_size(ctr + v(x, 0.0), v(0.35, 1.9)), 1.0, col);
            }
            ln(v(0.35, -0.9), v(0.95, 0.9));
        }
        Ic::Vol => {
            p.rect_filled(Rect::from_center_size(ctr + v(-0.85, 0.0), v(0.5, 0.8)), 1.0, col);
            tri(v(-0.6, -0.4), v(0.1, -0.9), v(0.1, 0.9));
            for r in [0.5f32, 0.95] {
                let pts: Vec<Pos2> = (0..=8)
                    .map(|i| {
                        let a = -0.9 + 1.8 * i as f32 / 8.0;
                        ctr + v(0.15, 0.0) + vec2(a.cos(), a.sin()) * h * r
                    })
                    .collect();
                p.add(Shape::line(pts, st));
            }
        }
    }
}

fn hand(ui: &Ui, rs: &Response) {
    if rs.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

fn hov(ui: &Ui, rs: &Response) -> f32 {
    hov_if(ui, rs.id, rs.hovered())
}

fn hov_if(ui: &Ui, id: Id, on: bool) -> f32 {
    let t = ui.ctx().animate_bool_with_time(id.with("hov"), on, 0.14);
    t * t * (3.0 - 2.0 * t)
}

fn press(ui: &Ui, rs: &Response) -> f32 {
    ui.ctx()
        .animate_bool_with_time(rs.id.with("press"), rs.is_pointer_button_down_on(), 0.07)
}

fn pop(ui: &Ui, rs: &Response, r: Rect, grow: f32) -> Rect {
    r.expand(grow * hov(ui, rs) - 1.5 * press(ui, rs))
}

fn ibtn(ui: &mut Ui, ic: Ic, ctr: Pos2, s: f32, on: bool, id: &str) -> bool {
    let rs = ui.interact(
        Rect::from_center_size(ctr, vec2(s + 14.0, s + 14.0)),
        Id::new(id),
        Sense::click(),
    );
    let t = hov(ui, &rs);
    let col = if on { accent() } else { MUTED.lerp_to_gamma(TEXT, t) };

    icon(ui.painter(), ic, ctr, s * (1.0 + 0.18 * t - 0.10 * press(ui, &rs)), col);
    hand(ui, &rs);
    rs.clicked()
}

fn slider(ui: &mut Ui, rect: Rect, val: &mut f32, id: &str) {
    let rs = ui.interact(rect.expand2(vec2(0.0, 7.0)), Id::new(id), Sense::click_and_drag());
    if rs.clicked() || rs.dragged() {
        if let Some(pt) = rs.interact_pointer_pos() {
            *val = ((pt.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        }
    }
    let t = hov_if(ui, rs.id, rs.hovered() || rs.dragged());
    hand(ui, &rs);
    let p = ui.painter();

    let th = 4.0 + 2.0 * t;
    let tr = Rect::from_center_size(rect.center(), vec2(rect.width(), th));
    p.rect_filled(tr, th / 2.0, LINE);
    p.rect_filled(
        Rect::from_min_size(tr.min, vec2(tr.width() * *val, th)),
        th / 2.0,
        TEXT.lerp_to_gamma(accent(), t),
    );
    if t > 0.003 {
        p.circle_filled(pos2(rect.left() + rect.width() * *val, rect.center().y), 5.5 * t, TEXT);
    }
}

fn wave(ui: &mut Ui, r: Rect, prog: &mut f32, seed: usize, secs: f32) -> (bool, bool) {
    wave_id(ui, r, prog, seed, "seek", secs)
}

fn wave_id(ui: &mut Ui, r: Rect, prog: &mut f32, seed: usize, id: &str, secs: f32) -> (bool, bool) {
    let rs = ui.interact(r.expand2(vec2(0.0, 8.0)), Id::new(id), Sense::click_and_drag());
    if rs.clicked() || rs.dragged() {
        if let Some(pt) = rs.interact_pointer_pos() {
            *prog = ((pt.x - r.left()) / r.width()).clamp(0.0, 1.0);
        }
    }
    hand(ui, &rs);

    let hot = rs.hovered() || rs.dragged();
    let t = hov_if(ui, rs.id, hot);
    let mid = rs.id.with("mx");
    if hot {
        if let Some(pt) = ui.input(|i| i.pointer.hover_pos()) {
            ui.ctx().memory_mut(|m| m.data.insert_temp(mid, pt.x));
        }
    }
    let mx = ui.ctx().memory(|m| m.data.get_temp::<f32>(mid)).unwrap_or(-1.0e4);
    let n = ((r.width() / 5.0) as usize).max(1);
    let sd = seed as f32;
    let front = r.left() + *prog * r.width();
    let p = ui.painter();
    for k in 0..n {
        let f = k as f32;
        let bx = r.left() + f * 5.0 + 1.5;
        let near = (-((bx - mx) / 34.0).powi(2)).exp() * t;
        let hh = (0.25 + 0.75 * ((f * 0.45 + sd * 1.7).sin() * (f * 0.17 + sd).cos()).abs()).clamp(0.2, 1.0)
            * r.height()
            * (1.0 + 0.4 * near);

        let bar = Rect::from_center_size(pos2(bx, r.center().y), vec2(3.0 + near, hh));
        p.rect_filled(bar, 0.0, LINE.lerp_to_gamma(MUTED, if bx <= mx { t } else { 0.0 }));
        let fill = ((front - bar.left()) / bar.width()).clamp(0.0, 1.0);
        if fill > 0.0 {
            p.rect_filled(
                Rect::from_min_size(bar.min, vec2(bar.width() * fill, hh)),
                0.0,
                accent(),
            );
        }
    }

    if secs > 0.0 && (rs.hovered() || rs.dragged()) {
        if let Some(pt) = ui.input(|i| i.pointer.hover_pos()) {
            let px = pt.x.clamp(r.left(), r.right());
            let t = ((px - r.left()) / r.width()).clamp(0.0, 1.0) * secs;
            ui.painter().line_segment(
                [pos2(px, r.top() - 2.0), pos2(px, r.bottom() + 2.0)],
                Stroke::new(1.5_f32, TEXT.gamma_multiply(0.75)),
            );
            let top = ui
                .ctx()
                .layer_painter(LayerId::new(Order::Tooltip, Id::new("wave_tip")));
            let g = top.layout_no_wrap(mmss(t), FontId::proportional(11.0), TEXT);
            let (bw, bh) = (g.size().x + 14.0, g.size().y + 8.0);
            let sr = ui.ctx().screen_rect();
            let cx = px.clamp(sr.left() + bw / 2.0 + 4.0, sr.right() - bw / 2.0 - 4.0);
            let br = Rect::from_center_size(pos2(cx, r.top() - 8.0 - bh / 2.0), vec2(bw, bh));
            top.rect_filled(br, 6.0, SURF2);
            top.rect_stroke(br, 6.0, Stroke::new(1.0_f32, LINE));
            top.galley(br.center() - g.size() / 2.0, g, TEXT);
        }
    }
    (rs.dragged(), rs.clicked() || rs.drag_stopped())
}

fn wave_bars(p: &Painter, r: Rect, prog: f32, seed: usize) {
    let n = ((r.width() / 5.0) as usize).max(1);
    let sd = seed as f32;
    let front = r.left() + prog * r.width();
    for k in 0..n {
        let f = k as f32;
        let hh =
            (0.25 + 0.75 * ((f * 0.45 + sd * 1.7).sin() * (f * 0.17 + sd).cos()).abs()).clamp(0.2, 1.0) * r.height();
        let bar = Rect::from_center_size(pos2(r.left() + f * 5.0 + 1.5, r.center().y), vec2(3.0, hh));
        p.rect_filled(bar, 0.0, LINE);
        let fill = ((front - bar.left()) / bar.width()).clamp(0.0, 1.0);
        if fill > 0.0 {
            p.rect_filled(
                Rect::from_min_size(bar.min, vec2(bar.width() * fill, hh)),
                0.0,
                accent(),
            );
        }
    }
}

fn marquee(ui: &Ui, p: &Painter, clip: Rect, y: f32, text: &str, font: FontId, col: Color32) {
    let tp = p.with_clip_rect(clip);
    let w = p.layout_no_wrap(text.to_string(), font.clone(), col).size().x;
    if w <= clip.width() {
        tp.text(pos2(clip.left(), y), Align2::LEFT_CENTER, text, font, col);
        return;
    }
    let (gap, speed, pause) = (40.0_f32, 35.0_f32, 1.5_f32);
    let period = (w + gap) / speed;
    let t = (ui.input(|i| i.time) % (period + pause) as f64) as f32;
    let off = if t < pause { 0.0 } else { (t - pause) * speed };
    for k in 0..2 {
        tp.text(
            pos2(clip.left() - off + k as f32 * (w + gap), y),
            Align2::LEFT_CENTER,
            text,
            font.clone(),
            col,
        );
    }
    ui.ctx().request_repaint();
}

fn mmss(s: f32) -> String {
    format!("{}:{:02}", (s as u32) / 60, (s as u32) % 60)
}

#[derive(Default)]
struct Caret {
    x: Option<f32>,
    target: f32,
    last_move: f64,
}

fn text_field(ui: &mut Ui, rect: Rect, id: Id, text: &mut String, hint: &str, caret: &mut Caret) {
    text_field_sized(ui, rect, id, text, hint, caret, 15.0);
}

fn text_field_sized(ui: &mut Ui, rect: Rect, id: Id, text: &mut String, hint: &str, caret: &mut Caret, size: f32) {
    let mut target: Option<(f32, f32, f32)> = None;
    ui.allocate_new_ui(UiBuilder::new().max_rect(rect), |ui| {
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let out = TextEdit::singleline(&mut *text)
                .id(id)
                .hint_text(RichText::new(hint).color(MUTED))
                .frame(false)
                .margin(vec2(0.0, 0.0))
                .text_color(TEXT)
                .font(FontId::proportional(size))
                .desired_width(rect.width())
                .show(ui);
            if out.response.has_focus() {
                if let Some(cr) = out.cursor_range {
                    let rc = out.galley.pos_from_cursor(&cr.primary);
                    target = Some((
                        out.galley_pos.x + rc.min.x,
                        out.galley_pos.y + rc.min.y,
                        out.galley_pos.y + rc.max.y,
                    ));
                }
            }
        });
    });

    let now = ui.input(|i| i.time);
    let dt = ui.input(|i| i.stable_dt);
    match target {
        Some((tx, y0, y1)) => {
            let cx = caret.x.get_or_insert(tx);
            if (tx - caret.target).abs() > 0.01 {
                caret.target = tx;
                caret.last_move = now;
            }
            let diff = tx - *cx;
            let moving = diff.abs() > 0.2;
            if moving {
                *cx += diff * (1.0 - (-dt * 28.0).exp());
                ui.ctx().request_repaint();
            } else {
                *cx = tx;
            }

            let on = moving || (now - caret.last_move) % 1.06 < 0.53;
            if on {
                ui.painter().rect_filled(
                    Rect::from_min_max(pos2(*cx - 1.0, y0 + 1.0), pos2(*cx + 1.0, y1 - 1.0)),
                    1.0,
                    accent(),
                );
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
        }
        None => caret.x = None,
    }
}

fn spaced(p: &Painter, pos: Pos2, al: Align2, text: &str, size: f32, col: Color32) {
    let f = FontId::proportional(size);
    let sp = size * 0.14;
    let ws: Vec<f32> = text
        .chars()
        .map(|ch| p.layout_no_wrap(ch.to_string(), f.clone(), col).size().x)
        .collect();
    let total = ws.iter().sum::<f32>() + sp * ws.len().saturating_sub(1) as f32;
    let mut x = match al.x() {
        Align::RIGHT => pos.x - total,
        Align::Center => pos.x - total / 2.0,
        _ => pos.x,
    };
    for (ch, w) in text.chars().zip(ws) {
        p.text(pos2(x, pos.y), Align2::LEFT_CENTER, ch.to_string(), f.clone(), col);
        x += w + sp;
    }
}

fn top(ui: &mut Ui, w: f32, label: &str, title: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(w, 76.0), Sense::hover());
    let p = ui.painter();
    spaced(
        p,
        pos2(r.left(), r.top() + 14.0),
        Align2::LEFT_CENTER,
        label,
        11.0,
        accent(),
    );
    p.text(
        pos2(r.left(), r.top() + 46.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(30.0),
        TEXT,
    );
}

fn section(ui: &mut Ui, w: f32, title: &str, right: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
    let p = ui.painter();
    p.text(
        r.left_center(),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(18.0),
        TEXT,
    );
    spaced(p, r.right_center(), Align2::RIGHT_CENTER, right, 11.0, MUTED);
}

fn section_btn(ui: &mut Ui, w: f32, title: &str, right: &str, id: &str, play: bool) -> bool {
    let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
    let f = FontId::proportional(11.0);
    let sp = 11.0 * 0.14;
    let total: f32 = right
        .chars()
        .map(|ch| ui.painter().layout_no_wrap(ch.to_string(), f.clone(), MUTED).size().x)
        .sum::<f32>()
        + sp * right.chars().count().saturating_sub(1) as f32;
    let extra = if play { 20.0 } else { 0.0 };
    let br = Rect::from_min_max(
        pos2(r.right() - total - extra - 12.0, r.top() + 6.0),
        pos2(r.right(), r.bottom() - 6.0),
    );
    let rs = ui.interact(br, Id::new(id), Sense::click());
    let t = hov(ui, &rs);
    let col = MUTED.lerp_to_gamma(accent(), t);
    let p = ui.painter();
    p.text(
        r.left_center(),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(18.0),
        TEXT,
    );
    spaced(p, r.right_center(), Align2::RIGHT_CENTER, right, 11.0, col);
    if play {
        icon(
            p,
            Ic::Play,
            pos2(r.right() - total - 10.0, r.center().y),
            10.0 + 3.0 * t,
            col,
        );
    }
    hand(ui, &rs);
    rs.clicked()
}

fn skel(ui: &Ui, r: Rect, rd: f32) {
    let t = ui.input(|i| i.time) as f32;
    let k = 0.5 + 0.5 * (t * 3.0).sin();
    ui.painter().rect_filled(r, rd, SURF.lerp_to_gamma(SURF2, k));
    ui.ctx().request_repaint();
}

fn acrylic_panel(p: &Painter, r: Rect, rd: f32) {
    p.rect_filled(r, rd, MENU);
    if r.height() < 8.0 || r.width() < 8.0 {
        return;
    }

    p.line_segment(
        [pos2(r.left() + rd, r.top() + 0.5), pos2(r.right() - rd, r.top() + 0.5)],
        Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(34, 34, 34, 34)),
    );
}

fn chips(ui: &mut Ui, w: f32, labels: &[&str], sel: &mut usize, tag: &str) {
    let (cr, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
    let mut x = cr.left();
    for (i, l) in labels.iter().enumerate() {
        let g = ui
            .painter()
            .layout_no_wrap(l.to_string(), FontId::proportional(13.0), TEXT);
        let br = Rect::from_min_size(pos2(x, cr.top() + 3.0), vec2(g.size().x + 26.0, 28.0));
        let rs = ui.interact(br, Id::new((tag, i)), Sense::click());
        let on = *sel == i;
        let t = hov(ui, &rs);
        let bz = pop(ui, &rs, br, 1.5);
        let p = ui.painter();
        if on {
            p.rect_filled(bz, 6.0, TEXT);
        } else {
            p.rect_filled(bz, 6.0, SURF.gamma_multiply(t));
            p.rect_stroke(bz, 6.0, Stroke::new(1.0_f32, LINE.lerp_to_gamma(MUTED, t)));
        }
        p.text(
            br.center(),
            Align2::CENTER_CENTER,
            l,
            FontId::proportional(13.0),
            if on { BG } else { TEXT },
        );
        hand(ui, &rs);
        if rs.clicked() {
            *sel = i;
        }
        x = br.right() + 6.0;
    }
}

impl eframe::App for App {
    fn clear_color(&self, _v: &Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &Context, _f: &mut eframe::Frame) {
        if self.glass_tries < 240 {
            self.glass_tries += 1;
            if let Some(ok) = win_glass::apply() {
                GLASS_ON.store(ok, AtOrd::Relaxed);
                self.glass_tries = 1000;
                ctx.request_repaint();
            }
        } else {
            win_glass::strip_sysmenu();
        }
        let acc = ACCENT_RGB.load(AtOrd::Relaxed);
        if acc != self.acc_applied {
            self.acc_applied = acc;
            ctx.style_mut(|st| st.visuals.selection.bg_fill = accent());
        }
        self.imgs.poll(ctx);
        self.audio_tick(ctx);
        self.discord_sync(ctx);
        self.dep_tick(ctx);
        self.mini_anim_step(ctx);
        self.exit_anim_step(ctx);
        self.switch_step(ctx);

        while let Ok((q, res)) = self.home_rx.try_recv() {
            self.home_pending.remove(&q);
            match res {
                Ok(h) => {
                    self.home_cache.insert(q, Arc::new(h));
                }
                Err(e) => {
                    self.home_err.insert(q, e);
                }
            }
        }

        while let Ok((id, res)) = self.ly_rx.try_recv() {
            let slot = match res {
                LyRes::Found(l) => LySlot::Ready(Arc::new(l)),
                LyRes::NotFound => LySlot::Missing,
                LyRes::Error => LySlot::Error,
            };
            self.ly.insert(id, slot);
        }

        if let Some(rx) = &self.rx {
            if let Ok((q, res)) = rx.try_recv() {
                if q == self.sent_query {
                    match res {
                        Ok(h) => {
                            self.hits = h;
                            self.search_err = None;
                        }
                        Err(e) => {
                            self.hits.clear();
                            self.search_err = Some(e);
                        }
                    }
                    self.searching = false;
                }
            }
        }

        let sr = ctx.screen_rect();
        let (min_w, min_h) = if self.mini { (200.0, 80.0) } else { (400.0, 300.0) };
        if sr.width() < min_w || sr.height() < min_h {
            return;
        }
        CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| self.root(ui));
    }
}

impl App {
    fn tick_search(&mut self, ctx: &Context) {
        let now = ctx.input(|i| i.time);
        if self.query != self.seen_query {
            if self.query.trim().is_empty() && !self.seen_query.trim().is_empty() {
                self.sel = None;
            }
            self.seen_query = self.query.clone();
            self.edit_t = now;
        }
        let q = self.query.trim().to_string();
        if q.is_empty() {
            self.hits.clear();
            self.searching = false;
            self.search_err = None;
            self.sent_query.clear();
            return;
        }
        if q != self.sent_query {
            if now - self.edit_t >= 0.4 {
                self.sent_query = q.clone();
                self.searching = true;
                let (tx, rx) = channel();
                self.rx = Some(rx);
                let ctx2 = ctx.clone();
                std::thread::spawn(move || {
                    let res = yt_search(&q);
                    let _ = tx.send((q, res));
                    ctx2.request_repaint();
                });
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }

    fn push_recent(&mut self, q: &str) {
        let q = q.trim();
        if q.is_empty() {
            return;
        }
        self.recent.retain(|r| !r.eq_ignore_ascii_case(q));
        self.recent.insert(0, q.to_string());
        self.recent.truncate(MAX_RECENT);
        save_recent(&self.recent);
    }

    fn push_history(&mut self, h: &Hit) {
        self.history.retain(|x| x.id != h.id);
        self.history.insert(0, h.clone());
        self.history.truncate(MAX_HISTORY);
        save_history(&self.history);
    }

    fn history_list(&mut self, ui: &mut Ui, w: f32) {
        if self.history.is_empty() {
            return;
        }
        ui.add_space(14.0);
        let p = ui.painter().clone();
        let (hr, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        p.text(
            hr.left_center(),
            Align2::LEFT_CENTER,
            tr(self.language, "recent_title"),
            FontId::proportional(18.0),
            TEXT,
        );
        let cb = Rect::from_center_size(pos2(hr.right() - 36.0, hr.center().y), vec2(80.0, 30.0));
        let crs = ui.interact(cb, Id::new("history_clear"), Sense::click());
        spaced(
            &p,
            hr.right_center(),
            Align2::RIGHT_CENTER,
            tr(self.language, "clear"),
            11.0,
            MUTED.lerp_to_gamma(TEXT, hov(ui, &crs)),
        );
        hand(ui, &crs);
        if crs.clicked() {
            self.history.clear();
            save_history(&self.history);
            return;
        }

        let items: Vec<Hit> = self.history.iter().take(6).cloned().collect();
        let mut act: Option<(Row, Hit, Rect)> = None;
        for (i, h) in items.iter().enumerate() {
            let (r, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
            let rr = Rect::from_min_size(r.min, vec2(w, 52.0));
            let on = self.sel.as_ref().is_some_and(|s| s.id == h.id);
            if let Some(a) = Self::hit_row(
                ui,
                rr,
                h,
                i,
                on,
                &self.imgs,
                false,
                self.liked_ids.contains(&h.id),
                false,
            ) {
                act = Some((a, h.clone(), rr));
            }
        }
        if let Some((a, h, rr)) = act {
            if self.row_common(a, &h, rr) == Some(Row::Open) {
                self.sel = Some(h.clone());
                self.push_history(&h);
            }
        }
    }

    fn row_common(&mut self, a: Row, h: &Hit, r: Rect) -> Option<Row> {
        match a {
            Row::Like => {
                self.toggle_like_hit(h);
                None
            }
            Row::Add => {
                self.pl_menu = Some(PlMenu {
                    hit: h.clone(),
                    at: pos2(r.right() - ADD_DX, r.center().y),
                });
                None
            }
            other => Some(other),
        }
    }

    fn recent_dropdown(&mut self, ui: &mut Ui, bar: Rect, focused: bool, sid: Id) {
        let q = self.query.trim().to_lowercase();
        let items: Vec<String> = self
            .recent
            .iter()
            .filter(|s| {
                let l = s.to_lowercase();
                q.is_empty() || (l.contains(&q) && l != q)
            })
            .take(6)
            .cloned()
            .collect();

        let over = self
            .drop_rect
            .is_some_and(|r| ui.input(|i| i.pointer.hover_pos().is_some_and(|p| r.contains(p))));
        let open = !items.is_empty() && (focused || (self.drop_open && over));
        self.drop_open = open;

        let full = 36.0 + items.len() as f32 * 40.0 + 8.0;
        let h = ui
            .ctx()
            .animate_value_with_time(Id::new("drop_h"), if open { full } else { 0.0 }, 0.14);
        if h < 1.0 {
            self.drop_rect = None;
            return;
        }
        let r = Rect::from_min_size(bar.left_bottom() + vec2(0.0, 6.0), vec2(bar.width(), h));
        self.drop_rect = Some(r);
        ui.interact(r, Id::new("drop_block"), Sense::click());
        let p = ui.painter().with_clip_rect(r);
        acrylic_panel(&p, r, 10.0);
        p.rect_stroke(r.shrink(0.5), 9.5, Stroke::new(1.0_f32, LINE));

        spaced(
            &p,
            pos2(r.left() + 16.0, r.top() + 18.0),
            Align2::LEFT_CENTER,
            tr(self.language, "recent"),
            10.0,
            MUTED,
        );
        let cb = Rect::from_center_size(pos2(r.right() - 44.0, r.top() + 18.0), vec2(80.0, 26.0));
        let crs = ui.interact(cb, Id::new("drop_clear"), Sense::click());
        spaced(
            &p,
            pos2(r.right() - 16.0, r.top() + 18.0),
            Align2::RIGHT_CENTER,
            tr(self.language, "clear"),
            10.0,
            MUTED.lerp_to_gamma(TEXT, hov(ui, &crs)),
        );
        hand(ui, &crs);

        let (mut run, mut del): (Option<String>, Option<String>) = (None, None);
        for (k, s) in items.iter().enumerate() {
            let rr = Rect::from_min_size(
                pos2(r.left() + 6.0, r.top() + 36.0 + k as f32 * 40.0),
                vec2(r.width() - 12.0, 40.0),
            );
            if rr.bottom() > r.bottom() + 0.5 {
                break;
            }
            let rs = ui.interact(rr, Id::new(("drop_row", k)), Sense::click());
            let xc = pos2(rr.right() - 22.0, rr.center().y);
            let xs = ui.interact(
                Rect::from_center_size(xc, vec2(26.0, 26.0)),
                Id::new(("drop_x", k)),
                Sense::click(),
            );
            let hot = rs.hovered() || xs.hovered();
            let ht = hov_if(ui, rs.id, hot);
            let xt = hov(ui, &xs);
            if ht > 0.003 {
                p.rect_filled(rr, 8.0, LINE.gamma_multiply(ht));
            }
            icon(
                &p,
                Ic::Clock,
                pos2(rr.left() + 22.0, rr.center().y),
                14.0,
                MUTED.lerp_to_gamma(TEXT, ht),
            );
            fit(
                &p,
                pos2(rr.left() + 44.0, rr.center().y),
                s,
                FontId::proportional(14.0),
                TEXT,
                rr.width() - 44.0 - 48.0,
            );
            if ht > 0.003 {
                let st = Stroke::new(1.6_f32, MUTED.lerp_to_gamma(TEXT, xt).gamma_multiply(ht));
                let z = 4.0 * (1.0 + 0.25 * xt);
                p.line_segment([xc + vec2(-z, -z), xc + vec2(z, z)], st);
                p.line_segment([xc + vec2(-z, z), xc + vec2(z, -z)], st);
            }
            hand(ui, &rs);
            if xs.clicked() {
                del = Some(s.clone());
            } else if rs.clicked() {
                run = Some(s.clone());
            }
        }

        if crs.clicked() {
            self.recent.clear();
            save_recent(&self.recent);
        }
        if let Some(s) = del {
            self.recent.retain(|x| x != &s);
            save_recent(&self.recent);
        }
        if let Some(s) = run {
            self.query = s.clone();
            self.seen_query = s.clone();
            self.edit_t = ui.input(|i| i.time) - 1.0;
            self.push_recent(&s);
            self.drop_open = false;
            ui.memory_mut(|m| m.surrender_focus(sid));
            ui.ctx().request_repaint();
        }
    }

    fn now(&self) -> Hit {
        self.queue.get(self.qi).cloned().unwrap_or_default()
    }

    fn now_id(&self) -> Option<String> {
        if self.started {
            self.queue.get(self.qi).map(|h| h.id.clone())
        } else {
            None
        }
    }

    fn now_seed(&self) -> usize {
        self.queue.get(self.qi).map(|h| seed_of(&h.id)).unwrap_or(0)
    }

    fn now_secs(&self) -> f32 {
        if let Some(d) = self.audio.dur {
            return d;
        }
        let s = self.queue.get(self.qi).map(|h| h.secs).unwrap_or(0);
        if s == 0 {
            200.0
        } else {
            s as f32
        }
    }

    fn is_liked(&self) -> bool {
        self.started && self.queue.get(self.qi).is_some_and(|h| self.liked_ids.contains(&h.id))
    }

    fn toggle_like_hit(&mut self, h: &Hit) {
        if self.liked_ids.remove(&h.id) {
            self.likes.retain(|x| x.id != h.id);
        } else {
            self.liked_ids.insert(h.id.clone());
            self.likes.insert(0, h.clone());
        }
        save_likes(&self.likes);
    }

    fn toggle_like(&mut self) {
        if !self.started {
            return;
        }
        if let Some(h) = self.queue.get(self.qi).cloned() {
            self.toggle_like_hit(&h);
        }
    }

    fn play_list(&mut self, list: Vec<Hit>, i: usize) {
        if list.is_empty() {
            return;
        }
        let i = i.min(list.len() - 1);
        let h = list[i].clone();
        self.queue = list;
        self.qi = i;
        self.started = true;
        self.progress = 0.0;
        self.playing = true;
        self.shuf_next = None;
        self.push_history(&h);
        self.load_current();
    }

    fn load_current(&mut self) {
        self.audio.stop();
        if let Some(h) = self.queue.get(self.qi) {
            let id = h.id.clone();
            self.audio.fetch(id);
        }
    }

    fn audio_tick(&mut self, ctx: &Context) {
        let dt = ctx.input(|i| i.stable_dt);
        let mut retry = false;
        while let Ok((g, res)) = self.audio.rx.try_recv() {
            if g != self.audio.gen {
                continue;
            }
            self.audio.loading = false;
            let at = self.progress * self.now_secs();
            let r = match res {
                Ok(p) => self.audio.start(&p, self.playing, self.volume, at),
                Err(e) => Err(e),
            };
            match r {
                Ok(()) => self.auto_retry = None,
                Err(e) => {
                    let cur = self.now_id();
                    if cur.is_some() && self.auto_retry != cur {
                        self.auto_retry = cur;
                        retry = true;
                    } else {
                        self.audio.err = Some(e);
                        self.playing = false;
                    }
                }
            }
        }
        if retry {
            self.load_current();
        }

        let mut reload = false;
        while let Ok((g, at, r)) = self.audio.seek_rx.try_recv() {
            if g != self.audio.seek_gen {
                continue;
            }
            match (r, self.audio.path.clone()) {
                (Ok(src), Some(p)) => {
                    if self.audio.install(src, &p, self.playing, self.volume, at).is_ok() && !self.seeking {
                        self.progress = (at / self.now_secs()).clamp(0.0, 1.0);
                    }
                }
                _ => reload = true,
            }
        }
        if reload {
            self.load_current();
        }
        if self.playing && !self.last_playing {
            self.audio.err = None;
            self.auto_retry = None;
        }
        self.last_playing = self.playing;

        if self.started && self.playing && !self.audio.loading && self.audio.sink.is_none() && self.audio.err.is_none()
        {
            self.load_current();
        }
        if let Some(s) = &self.audio.sink {
            s.set_volume(self.volume * self.volume);
            if self.playing && s.is_paused() {
                s.play();
            }
            if !self.playing && !s.is_paused() {
                s.pause();
            }
        }
        if self.playing && self.audio.sink.is_some() {
            let ended = self.audio.sink.as_ref().is_some_and(|s| s.empty());
            if ended {
                self.on_track_end();
            } else if !self.seeking {
                if self.audio.seek_pending {
                    self.progress = (self.progress + dt / self.now_secs()).min(1.0);
                } else {
                    self.progress = (self.audio.pos() / self.now_secs()).clamp(0.0, 1.0);
                }
            }
        }

        if self.audio.sink.is_some() {
            if self.playing {
                if self.audio.pos_since.is_none() {
                    self.audio.pos_since = Some(std::time::Instant::now());
                }
            } else if let Some(t0) = self.audio.pos_since.take() {
                self.audio.pos_base += t0.elapsed().as_secs_f32();
            }
        }

        if self.started && self.audio.sink.is_some() {
            if let Some(cur) = self.now_id() {
                if self.pf_for != cur {
                    self.pf_for = cur;
                    let ids = self.upcoming_ids(2);
                    cache_enqueue(&ids, true);
                }
            }
        }

        if self.started {
            if let Some(cur) = self.now_id() {
                if self.ly_for != cur {
                    self.ly_for = cur;
                    if let Some(h) = self.queue.get(self.qi).cloned() {
                        self.ly_request(ctx, &h);
                    }
                    for id in self.upcoming_ids(1) {
                        if let Some(h) = self.queue.iter().find(|x| x.id == id).cloned() {
                            self.ly_request(ctx, &h);
                        }
                    }
                }
            }
        }

        let t = ctx.input(|i| i.time);
        if t - self.last_sync > 5.0 {
            self.last_sync = t;
            self.sync_cache();
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(5));
        if self.playing || self.audio.loading {
            ctx.request_repaint();
        }
    }

    fn upcoming_ids(&mut self, n: usize) -> Vec<String> {
        let len = self.queue.len();
        if len <= 1 || self.repeat {
            return Vec::new();
        }
        if self.shuffle {
            let ok = self.shuf_next.is_some_and(|j| j < len && j != self.qi);
            if !ok {
                let j = rand_below(len);
                self.shuf_next = Some(if j == self.qi { (j + 1) % len } else { j });
            }
            return self
                .shuf_next
                .and_then(|j| self.queue.get(j))
                .map(|h| vec![h.id.clone()])
                .unwrap_or_default();
        }
        self.queue
            .iter()
            .skip(self.qi + 1)
            .take(n)
            .map(|h| h.id.clone())
            .collect()
    }

    fn sync_cache(&mut self) {
        let recent: Vec<String> = self.history.iter().take(HISTORY_KEEP).map(|h| h.id.clone()).collect();
        let liked: Vec<String> = self.likes.iter().map(|h| h.id.clone()).collect();
        let in_pl: Vec<String> = self
            .playlists
            .iter()
            .flat_map(|p| p.tracks.iter().map(|h| h.id.clone()))
            .collect();
        let mut prot: std::collections::HashSet<String> = std::collections::HashSet::new();
        prot.extend(recent.iter().cloned());
        prot.extend(liked.iter().cloned());
        prot.extend(in_pl.iter().cloned());
        prot.extend(self.queue.iter().skip(self.qi).take(10).map(|h| h.id.clone()));
        cache_lock().protected = prot;
        INIT_EVICT.call_once(|| {
            std::thread::spawn(cache_evict);
        });
        let mut order: Vec<String> = recent.into_iter().take(20).collect();
        order.extend(liked);
        order.extend(in_pl);
        cache_enqueue(&order, false);
    }

    fn on_track_end(&mut self) {
        self.progress = 0.0;
        if self.repeat {
            self.load_current();
        } else if self.shuffle || self.qi + 1 < self.queue.len() {
            self.step(1);
        } else {
            self.playing = false;
            self.audio.stop();
        }
    }

    fn play_or_toggle(&mut self, list: Vec<Hit>, i: usize) {
        let id = list.get(i).map(|h| h.id.clone());
        if id.is_some() && self.now_id() == id {
            self.playing = !self.playing;
        } else {
            self.play_list(list, i);
        }
    }

    fn card_list(&self, h: &Hit) -> (Vec<Hit>, usize) {
        match self.hits.iter().position(|x| x.id == h.id) {
            Some(i) => (self.hits.clone(), i),
            None => (vec![h.clone()], 0),
        }
    }

    fn queue_next(&mut self, h: &Hit) -> &'static str {
        if !self.started || self.queue.is_empty() {
            self.play_list(vec![h.clone()], 0);
            return tr(self.language, "play");
        }
        if self.now_id().as_deref() == Some(h.id.as_str()) {
            return match self.language {
                Language::En => "Already playing",
                Language::Fr => "Déjà en lecture",
                Language::It => "Già in riproduzione",
                Language::Es => "Ya se está reproduciendo",
                Language::Zh => "正在播放",
                Language::Ru => "Уже воспроизводится",
            };
        }
        if let Some(pos) = self.queue.iter().position(|x| x.id == h.id) {
            self.queue.remove(pos);
            if pos < self.qi {
                self.qi -= 1;
            }
        }
        self.queue.insert(self.qi + 1, h.clone());
        self.shuf_next = None;
        cache_enqueue(&[h.id.clone()], true);
        "Lu ensuite"
    }

    fn step(&mut self, d: isize) {
        if !self.started || self.queue.is_empty() {
            return;
        }

        if d < 0 && self.progress * self.now_secs() > 3.0 {
            self.progress = 0.0;
            self.audio.seek(0.0);
            return;
        }
        let len = self.queue.len();
        self.progress = 0.0;
        self.qi = if d > 0 && self.shuffle && len > 1 {
            let cur = self.qi;
            self.shuf_next
                .take()
                .filter(|&j| j < len && j != cur)
                .unwrap_or_else(|| {
                    let j = rand_below(len);
                    if j == cur {
                        (j + 1) % len
                    } else {
                        j
                    }
                })
        } else {
            (self.qi as isize + d).rem_euclid(len as isize) as usize
        };
        self.load_current();
    }

    fn home_load(&mut self, ctx: &Context, q: &str) {
        if self.home_cache.contains_key(q) || self.home_pending.contains(q) || self.home_err.contains_key(q) {
            return;
        }
        self.home_pending.insert(q.to_string());
        let (tx, ctx2, q2) = (self.home_tx.clone(), ctx.clone(), q.to_string());
        std::thread::spawn(move || {
            let r = yt_search(&q2);
            let _ = tx.send((q2, r));
            ctx2.request_repaint();
        });
    }

    fn toggle_play(&mut self) {
        if self.started {
            self.playing = !self.playing;
        } else if let Some(l) = self.home_cache.get(TOP_QUERY).cloned() {
            self.play_list(l.iter().take(10).cloned().collect(), 0);
        }
    }

    fn for_you_query(&self) -> String {
        if self.mood == 0 {
            self.fy_seed.clone()
        } else {
            MOOD_QUERIES[self.mood.min(MOOD_QUERIES.len() - 1)].to_string()
        }
    }

    fn title_bar(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        let ctx = ui.ctx().clone();

        let lc = pos2(r.left() + 22.0, r.center().y);
        for (k, hh) in [8.0f32, 15.0, 11.0].iter().enumerate() {
            p.rect_filled(
                Rect::from_center_size(lc + vec2((k as f32 - 1.0) * 5.0, 0.0), vec2(3.0, *hh)),
                1.0,
                accent(),
            );
        }
        spaced(
            &p,
            pos2(r.left() + 40.0, r.center().y),
            Align2::LEFT_CENTER,
            "RUSTIFY",
            11.0,
            MUTED,
        );

        let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
        let rad = if maximized { 0.0 } else { WIN_R };
        let bw = 46.0;

        let drag = Rect::from_min_max(r.min, pos2(r.right() - bw * 3.0, r.bottom()));
        let ds = ui.interact(drag, Id::new("titlebar_drag"), Sense::click_and_drag());
        if ds.double_clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        } else if ds.drag_started_by(PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        for k in 0..3usize {
            let b = Rect::from_min_max(
                pos2(r.right() - bw * (k as f32 + 1.0), r.top()),
                pos2(r.right() - bw * k as f32, r.bottom()),
            );
            let rs = ui.interact(b, Id::new(("wincap", k)), Sense::click());
            let close = k == 0;
            let t = hov(ui, &rs);
            if t > 0.003 {
                let rd = if close {
                    Rounding {
                        nw: 0.0,
                        ne: rad,
                        sw: 0.0,
                        se: 0.0,
                    }
                } else {
                    Rounding::ZERO
                };
                p.rect_filled(b, rd, (if close { accent() } else { SURF2 }).gamma_multiply(t));
            }
            let col = if close {
                MUTED.lerp_to_gamma(on_accent(), t)
            } else {
                MUTED.lerp_to_gamma(TEXT, t)
            };
            let st = Stroke::new(1.3_f32, col);
            let ctr = b.center();
            let z = 1.0 + 0.2 * t;
            match k {
                0 => {
                    p.line_segment([ctr + vec2(-5.0, -5.0) * z, ctr + vec2(5.0, 5.0) * z], st);
                    p.line_segment([ctr + vec2(-5.0, 5.0) * z, ctr + vec2(5.0, -5.0) * z], st);
                }
                1 => {
                    if maximized {
                        p.rect_stroke(
                            Rect::from_center_size(ctr + vec2(-1.5, 1.5) * z, vec2(8.0, 8.0) * z),
                            1.0,
                            st,
                        );
                        p.add(Shape::line(
                            vec![
                                ctr + vec2(-2.5, -2.5) * z,
                                ctr + vec2(-2.5, -6.5) * z,
                                ctr + vec2(5.5, -6.5) * z,
                                ctr + vec2(5.5, 1.5) * z,
                                ctr + vec2(2.5, 1.5) * z,
                            ],
                            st,
                        ));
                    } else {
                        p.rect_stroke(Rect::from_center_size(ctr, vec2(10.0, 10.0) * z), 1.0, st);
                    }
                }
                _ => {
                    p.line_segment([ctr + vec2(-5.0, 0.0) * z, ctr + vec2(5.0, 0.0) * z], st);
                }
            }
            if rs.clicked() {
                ctx.send_viewport_cmd(match k {
                    0 => ViewportCommand::Close,
                    1 => ViewportCommand::Maximized(!maximized),
                    _ => ViewportCommand::Minimized(true),
                });
            }
        }
    }

    fn enter_mini(&mut self, ctx: &Context) {
        if self.animating() {
            return;
        }
        let (size, outer, maxi, now) = ctx.input(|i| {
            (
                i.viewport().inner_rect.map(|r| r.size()),
                i.viewport().outer_rect,
                i.viewport().maximized.unwrap_or(false),
                i.time,
            )
        });
        let size = size.unwrap_or(vec2(1360.0, 840.0));
        self.restore = Some((size, outer.map(|r| r.min)));
        self.pl_menu = None;
        self.pl_ctx = None;
        self.sel = None;
        let to_pos = outer.map(|o| pos2(o.right() - MINI_W - 24.0, o.bottom() - MINI_H - 64.0));
        if maxi {
            self.mini = true;
            ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(300.0, 100.0)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(MINI_W, MINI_H)));
            ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            if let Some(p) = to_pos {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(p));
            }
            return;
        }
        self.mini_anim = Some(MiniAnim {
            t0: now,
            from_size: size,
            to_size: vec2(MINI_W, MINI_H),
            from_pos: outer.map(|r| r.min),
            to_pos,
        });
        ctx.request_repaint();
    }

    fn exit_mini(&mut self, ctx: &Context) {
        if self.animating() {
            return;
        }
        let (outer, now) = ctx.input(|i| (i.viewport().outer_rect, i.time));
        let (size, pos) = self.restore.take().unwrap_or((vec2(1360.0, 840.0), None));
        let Some(from) = outer else {
            self.mini = false;
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
            ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(MIN_W, MIN_H)));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            if let Some(p) = pos {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(p));
            }
            return;
        };
        let to = Rect::from_min_size(pos.unwrap_or(from.min), size);
        let uni = from.union(to);

        ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(300.0, 100.0)));

        self.exit_anim = Some(ExitAnim {
            t_start: now,
            t0: None,
            settle: None,
            fade0: None,
            from,
            to,
            uni,
        });
        ctx.request_repaint();
    }

    fn resize_handles(ui: &mut Ui, win: Rect) {
        use egui::viewport::ResizeDirection;
        let (b, c) = (5.0_f32, 12.0_f32);
        let (l, r, t, bt) = (win.left(), win.right(), win.top(), win.bottom());
        let zones = [
            (
                Rect::from_min_max(pos2(l + c, t), pos2(r - c, t + b)),
                ResizeDirection::North,
                CursorIcon::ResizeVertical,
            ),
            (
                Rect::from_min_max(pos2(l + c, bt - b), pos2(r - c, bt)),
                ResizeDirection::South,
                CursorIcon::ResizeVertical,
            ),
            (
                Rect::from_min_max(pos2(l, t + c), pos2(l + b, bt - c)),
                ResizeDirection::West,
                CursorIcon::ResizeHorizontal,
            ),
            (
                Rect::from_min_max(pos2(r - b, t + c), pos2(r, bt - c)),
                ResizeDirection::East,
                CursorIcon::ResizeHorizontal,
            ),
            (
                Rect::from_min_max(pos2(l, t), pos2(l + c, t + c)),
                ResizeDirection::NorthWest,
                CursorIcon::ResizeNwSe,
            ),
            (
                Rect::from_min_max(pos2(r - c, t), pos2(r, t + c)),
                ResizeDirection::NorthEast,
                CursorIcon::ResizeNeSw,
            ),
            (
                Rect::from_min_max(pos2(l, bt - c), pos2(l + c, bt)),
                ResizeDirection::SouthWest,
                CursorIcon::ResizeNeSw,
            ),
            (
                Rect::from_min_max(pos2(r - c, bt - c), pos2(r, bt)),
                ResizeDirection::SouthEast,
                CursorIcon::ResizeNwSe,
            ),
        ];
        for (k, (zr, dir, cur)) in zones.into_iter().enumerate() {
            let rs = ui.interact(zr, Id::new(("win_resize", k)), Sense::drag());
            if rs.hovered() || rs.dragged() {
                ui.ctx().set_cursor_icon(cur);
            }
            if rs.drag_started() {
                ui.ctx().send_viewport_cmd(ViewportCommand::BeginResize(dir));
            }
        }
    }

    fn animating(&self) -> bool {
        self.mini_anim.is_some() || self.exit_anim.is_some()
    }

    fn exit_geometry(ctx: &Context, r: Rect) {
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(r.min));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(r.size()));
    }

    fn exit_anim_step(&mut self, ctx: &Context) {
        let Some(mut a) = self.exit_anim else { return };
        ctx.request_repaint();
        let (now, outer) = ctx.input(|i| (i.time, i.viewport().outer_rect));
        let near = |r: Rect| {
            outer
                .is_some_and(|o| (o.min - r.min).abs().max_elem() < 4.0 && (o.size() - r.size()).abs().max_elem() < 4.0)
        };
        let sm = |x: f64| {
            let x = x.clamp(0.0, 1.0) as f32;
            x * x * (3.0 - 2.0 * x)
        };
        if let Some(f0) = a.fade0 {
            let t = now - f0;
            if t >= ANIM_FADE {
                self.exit_anim = None;
                self.fade_a = 0.0;
                return;
            }
            self.fade_a = 1.0 - sm(t / ANIM_FADE);
        } else if let Some(ts) = a.settle {
            if near(a.to) || now - ts > 0.3 {
                self.mini = false;
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::Normal));
                ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(MIN_W, MIN_H)));
                a.fade0 = Some(now);
                self.fade_a = 1.0;
            } else {
                Self::exit_geometry(ctx, a.to);
            }
        } else if let Some(t0) = a.t0 {
            if now - t0 >= ANIM_MOVE {
                a.settle = Some(now);
                Self::exit_geometry(ctx, a.to);
            }
        } else if near(a.uni) || now - a.t_start > 0.3 {
            a.t0 = Some(now);
        } else {
            Self::exit_geometry(ctx, a.uni);
        }
        self.exit_anim = Some(a);
    }

    fn exit_view(&mut self, ui: &mut Ui) {
        let Some(a) = self.exit_anim else { return };
        let ctx = ui.ctx().clone();
        let (now, outer) = ctx.input(|i| (i.time, i.viewport().outer_rect));
        ui.interact(ui.max_rect(), Id::new("mini_anim_block"), Sense::click_and_drag());
        if a.t0.is_none() {
            return;
        }
        let origin = outer.map(|o| o.min).unwrap_or(a.uni.min).to_vec2();
        let ease = |x: f64| {
            let x = x.clamp(0.0, 1.0) as f32;
            if x < 0.5 {
                4.0 * x * x * x
            } else {
                1.0 - (2.0 - 2.0 * x).powi(3) / 2.0
            }
        };
        let sm = |x: f64| {
            let x = x.clamp(0.0, 1.0) as f32;
            x * x * (3.0 - 2.0 * x)
        };
        let (e, t) = match (a.t0, a.settle) {
            (None, _) => (0.0, 0.0),
            (Some(t0), None) => (ease((now - t0) / ANIM_MOVE), now - t0),
            _ => (1.0, ANIM_MOVE),
        };
        let min = a.from.min + (a.to.min - a.from.min) * e;
        let max = a.from.max + (a.to.max - a.from.max) * e;
        let card = Rect::from_min_max(min, max).translate(-origin);
        let p = ui.painter().clone();
        p.rect_filled(card, WIN_R, BG);

        let mini_r = Rect::from_min_size(card.min, vec2(MINI_W, MINI_H));
        let fade = sm(t / (ANIM_MOVE * 0.5));
        if fade < 0.999 {
            ui.allocate_new_ui(UiBuilder::new().max_rect(mini_r), |ui| {
                ui.set_clip_rect(mini_r.intersect(card));
                self.mini_view(ui);
            });
            p.rect_filled(card, WIN_R, BG.gamma_multiply(fade));
        }
        p.rect_stroke(card.shrink(0.5), (WIN_R - 0.5).max(0.0), Stroke::new(1.0_f32, LINE));
    }

    fn mini_apply(ctx: &Context, a: &MiniAnim, e: f32) {
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(a.from_size + (a.to_size - a.from_size) * e));
        if let (Some(f), Some(t)) = (a.from_pos, a.to_pos) {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(f + (t - f) * e));
        }
    }

    fn mini_anim_step(&mut self, ctx: &Context) {
        let Some(a) = self.mini_anim else {
            if self.exit_anim.is_none() {
                self.fade_a = 0.0;
            }
            return;
        };
        let t = ctx.input(|i| i.time) - a.t0;
        let sm = |x: f64| {
            let x = x.clamp(0.0, 1.0) as f32;
            x * x * (3.0 - 2.0 * x)
        };
        let ease = |x: f64| {
            let x = x.clamp(0.0, 1.0) as f32;
            if x < 0.5 {
                4.0 * x * x * x
            } else {
                1.0 - (2.0 - 2.0 * x).powi(3) / 2.0
            }
        };
        ctx.request_repaint();
        if t < ANIM_FADE {
            self.fade_a = sm(t / ANIM_FADE);
        } else {
            let m = t - ANIM_FADE;
            if !self.mini {
                self.mini = true;
                ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(vec2(300.0, 100.0)));
                ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
                ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            }
            Self::mini_apply(ctx, &a, ease(m / ANIM_MOVE));
            self.fade_a = 1.0 - sm(m / (ANIM_MOVE * 0.6));
            if m >= ANIM_MOVE {
                Self::mini_apply(ctx, &a, 1.0);
                self.mini_anim = None;
                self.fade_a = 0.0;
            }
        }
    }

    fn ly_request(&mut self, ctx: &Context, h: &Hit) {
        if h.id.is_empty() || self.ly.contains_key(&h.id) {
            return;
        }
        let now = ctx.input(|i| i.time);
        self.ly.insert(h.id.clone(), LySlot::Loading(now));
        let (tx, ctx2, h2) = (self.ly_tx.clone(), ctx.clone(), h.clone());
        std::thread::spawn(move || {
            let r = fetch_lyrics(&h2);
            let _ = tx.send((h2.id.clone(), r));
            ctx2.request_repaint();
        });
    }

    fn lyrics_panel(&mut self, ui: &mut Ui, p: &Painter, area: Rect, fg: Color32, g: f32) {
        let Some(id) = self.now_id() else { return };
        let ctx = ui.ctx().clone();
        let now_t = ui.input(|i| i.time);
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let msg = |p: &Painter, text: &str, pulse: bool| {
            let a = if pulse {
                0.45 + 0.25 * (now_t as f32 * 3.0).sin()
            } else {
                0.6
            };
            p.text(
                area.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(15.0),
                fg.gamma_multiply(a * g),
            );
        };
        let (lyr, state) = match self.ly.get(&id) {
            Some(LySlot::Ready(l)) => (Some(l.clone()), 0),
            Some(LySlot::Loading(t0)) => (None, if now_t - *t0 > 25.0 { 3 } else { 1 }),
            Some(LySlot::Missing) => (None, 2),
            Some(LySlot::Error) => (None, 3),
            None => (None, 1),
        };
        let Some(lyr) = lyr else {
            match state {
                1 => {
                    msg(p, tr(self.language, "lyrics_search"), true);
                    ctx.request_repaint();
                }
                2 => msg(p, tr(self.language, "lyrics_none"), false),
                _ => {
                    msg(p, tr(self.language, "lyrics_error"), false);
                    let rs = ui.interact(area, Id::new("ly_retry"), Sense::click());
                    hand(ui, &rs);
                    if rs.clicked() {
                        self.ly.remove(&id);
                        let h = self.now();
                        self.ly_request(&ctx, &h);
                    }
                }
            }
            return;
        };
        if lyr.instrumental || lyr.lines.is_empty() {
            msg(p, "Instrumental", false);
            return;
        }
        let lines = &lyr.lines;
        let n = lines.len();
        let secs = self.now_secs();

        let off = 0.0_f32;
        let base_pos = if self.seeking || self.card_drag || self.audio.seek_pending || self.audio.sink.is_none() {
            self.progress * secs
        } else {
            self.audio.pos()
        };
        let pos = base_pos + 0.25 - off;
        let cur_i = lines.partition_point(|(t, _)| *t <= pos).checked_sub(1);
        let sm = |x: f32| {
            let x = x.clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };

        let show_map = area.width() >= 560.0;
        let mw = if show_map {
            (area.width() * 0.30).clamp(150.0, 280.0)
        } else {
            0.0
        };
        let ctl_h = 28.0;
        let lr = Rect::from_min_max(
            area.min,
            pos2(
                if show_map {
                    area.right() - mw - 32.0
                } else {
                    area.right()
                },
                area.bottom() - ctl_h,
            ),
        );
        let mr = Rect::from_min_max(
            pos2(area.right() - mw, area.top() + 2.0),
            pos2(area.right(), area.bottom() - 2.0),
        );

        let ww = (lr.width() - 6.0).max(80.0);

        let text_of = |s: &str| {
            if s.is_empty() {
                "[ · · · ]".to_string()
            } else {
                s.to_string()
            }
        };
        let lfont = |s: &str| FontId::proportional(if s.is_empty() { 16.0 } else { 24.0 });
        if self.ly_h.0 != id || self.ly_h.1 != ww as u32 || self.ly_h.2.len() != n {
            let hs: Vec<f32> = lines
                .iter()
                .map(|(_, s)| p.layout(text_of(s), lfont(s), Color32::WHITE, ww).size().y)
                .collect();
            self.ly_h = (id.clone(), ww as u32, hs);
        }
        let hs = self.ly_h.2.clone();
        let gap = 18.0;
        let mut ys = Vec::with_capacity(n);
        let mut acc = 0.0;
        for h in &hs {
            ys.push(acc);
            acc += h + gap;
        }

        let ti = cur_i.unwrap_or(0).min(n - 1);
        let target = ys[ti] + hs[ti] / 2.0;
        if self.ly_track != id || g < 0.6 {
            self.ly_track = id.clone();
            self.ly_y = target;
        }
        let diff = target - self.ly_y;
        if diff.abs() > 0.3 {
            self.ly_y += diff * (1.0 - (-dt * 7.0).exp());
            ctx.request_repaint();
        } else {
            self.ly_y = target;
        }

        let lp = p.with_clip_rect(lr.intersect(p.clip_rect()));
        let cy0 = lr.center().y;
        let half = (lr.height() / 2.0).max(1.0);
        let mut seek: Option<f32> = None;
        for i in 0..n {
            let sc = cy0 + (ys[i] + hs[i] / 2.0 - self.ly_y);
            let bx = Rect::from_min_size(pos2(lr.left(), sc - hs[i] / 2.0), vec2(lr.width(), hs[i]));
            if bx.bottom() < lr.top() || bx.top() > lr.bottom() {
                continue;
            }
            let dist = (sc - cy0).abs();
            let act = sm(1.0 - dist / (hs[i] / 2.0 + 30.0));
            let far = sm(dist / half);
            let edge = 1.0 - sm((dist / half - 0.62) / 0.38);
            let rs = ui.interact(bx.intersect(lr), Id::new(("ly_line", i)), Sense::click());
            let hv = hov(ui, &rs);
            let base = 0.50 * (1.0 - far) + 0.10;
            let a0 = base + (1.0 - base) * act;
            let a = (a0 + (0.85 - a0).max(0.0) * hv) * edge * g * if lines[i].1.is_empty() { 0.75 } else { 1.0 };
            let blur = sm((dist - 60.0) / (half - 60.0).max(1.0)) * (1.0 - hv);
            let gal = p.layout(text_of(&lines[i].1), lfont(&lines[i].1), Color32::WHITE, ww);
            let at = bx.min;
            if blur < 0.05 {
                lp.galley_with_override_text_color(at, gal, fg.gamma_multiply(a));
            } else {
                let r = 0.6 + 2.2 * blur;
                lp.galley_with_override_text_color(at, gal.clone(), fg.gamma_multiply(a * (1.0 - 0.5 * blur)));
                for d in [vec2(r, 0.0), vec2(-r, 0.0), vec2(0.0, r), vec2(0.0, -r)] {
                    lp.galley_with_override_text_color(at + d, gal.clone(), fg.gamma_multiply(a * 0.20 * blur));
                }
            }
            hand(ui, &rs);
            if rs.clicked() {
                seek = Some(lines[i].0);
            }
        }
        if !lyr.synced {
            p.text(
                pos2(lr.right(), area.bottom() - ctl_h / 2.0),
                Align2::RIGHT_CENTER,
                tr(self.language, "sync_approx"),
                FontId::proportional(10.0),
                fg.gamma_multiply(0.45 * g),
            );
        }

        if show_map {
            let rh = (mr.height() / n as f32).clamp(6.5, 15.0);
            let m = ((mr.height() / rh) as usize).clamp(1, n);
            let start = cur_i.unwrap_or(0).saturating_sub(m / 2).min(n - m);
            let fs = (rh * 0.82).clamp(5.5, 11.0);
            let mp = p.with_clip_rect(mr.expand2(vec2(14.0, 0.0)).intersect(p.clip_rect()));

            let hover_j = ui
                .input(|i| i.pointer.hover_pos())
                .filter(|pt| mr.expand2(vec2(6.0, 0.0)).contains(*pt))
                .map(|pt| (start + ((pt.y - mr.top()) / rh).max(0.0) as usize).min(start + m - 1));
            for j in start..start + m {
                let y = mr.top() + (j - start) as f32 * rh;
                let row = Rect::from_min_size(pos2(mr.left(), y), vec2(mw, rh));
                let is_cur = cur_i == Some(j);
                let ht = hov_if(ui, Id::new(("ly_map_row", j)), hover_j == Some(j));
                if ht > 0.003 && !is_cur {
                    mp.rect_filled(row.expand2(vec2(5.0, 0.0)), 3.0, fg.gamma_multiply(0.10 * ht * g));
                }
                if is_cur {
                    mp.rect_filled(row.expand2(vec2(5.0, 0.0)), 3.0, fg.gamma_multiply(0.14 * g));
                    mp.rect_filled(
                        Rect::from_center_size(pos2(row.left() - 8.0, row.center().y), vec2(3.0, (rh * 0.9).max(4.0))),
                        1.5,
                        fg.gamma_multiply(g),
                    );
                }
                let s = &lines[j].1;
                if s.is_empty() {
                    continue;
                }
                let a0 = if is_cur {
                    1.0
                } else if cur_i.map_or(true, |c| j > c) {
                    0.55
                } else {
                    0.38
                };
                let a = a0 + (0.95_f32 - a0).max(0.0) * ht;
                fit(
                    &mp,
                    pos2(row.left() + 2.0, row.center().y),
                    s,
                    FontId::proportional(fs),
                    fg.gamma_multiply(a * g),
                    mw - 6.0,
                );
            }
            let mrs = ui.interact(mr, Id::new("ly_map"), Sense::click());
            hand(ui, &mrs);
            if mrs.clicked() {
                if let Some(pt) = mrs.interact_pointer_pos() {
                    let j = (start + ((pt.y - mr.top()) / rh).max(0.0) as usize).min(n - 1);
                    seek = Some(lines[j].0);
                }
            }
        }

        if let Some(t) = seek {
            self.progress = (t / secs).clamp(0.0, 1.0);
            self.audio.seek(t);
        }
    }

    fn request_switch(&mut self, now: f64, who: Option<usize>) {
        if self.sw.is_some() {
            return;
        }
        self.sw = Some(SwitchAnim {
            t0: now,
            who,
            swapped: false,
        });
    }

    fn switch_step(&mut self, ctx: &Context) {
        let Some(mut a) = self.sw else { return };
        ctx.request_repaint();
        let now = ctx.input(|i| i.time);
        if !a.swapped {
            if now - a.t0 >= SW_OUT {
                self.switch_to(a.who);
                a.swapped = true;
                a.t0 = now;
                self.sw = Some(a);
            }
        } else if now - a.t0 >= SW_IN {
            self.sw = None;
        }
    }

    fn sw_overlay(&self, ui: &Ui, full: Rect, rad: f32) {
        let Some(a) = self.sw else { return };
        ui.interact(full, Id::new("sw_block"), Sense::click_and_drag());
        let t = ((ui.input(|i| i.time) - a.t0) / if a.swapped { SW_IN } else { SW_OUT }).clamp(0.0, 1.0) as f32;
        let t = t * t * (3.0 - 2.0 * t);
        let alpha = if a.swapped { 1.0 - t } else { t };
        if alpha > 0.003 {
            ui.painter().rect_filled(
                full,
                Rounding {
                    nw: 0.0,
                    ne: 0.0,
                    sw: rad,
                    se: rad,
                },
                BG.gamma_multiply(alpha),
            );
        }
    }

    fn anim_overlay(&self, ui: &Ui, win: Rect, rad: f32) {
        if self.animating() {
            ui.interact(win, Id::new("mini_anim_block"), Sense::click_and_drag());
        }
        if self.fade_a > 0.003 {
            ui.painter().rect_filled(win, rad, BG.gamma_multiply(self.fade_a));
        }
    }

    fn mini_view(&mut self, ui: &mut Ui) {
        let win = ui.max_rect();
        let ctx = ui.ctx().clone();
        let p = ui.painter().clone();
        p.rect_filled(win, WIN_R - 2.0, window_bg());
        let ds = ui.interact(win, Id::new("mini_drag"), Sense::click_and_drag());
        if ds.double_clicked() {
            self.exit_mini(&ctx);
            return;
        } else if ds.drag_started_by(PointerButton::Primary) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        let h = self.now();
        let seed = self.now_seed();
        let cov = Rect::from_min_size(win.min + vec2(12.0, 12.0), vec2(112.0, 112.0));
        if self.started {
            art(ui, &p, cov, &self.imgs, &[&h.thumb], seed, 8.0);
        } else {
            p.rect_filled(cov, 8.0, SURF);
        }

        let (title, artist) = if !self.started {
            (
                tr(self.language, "no_track").to_string(),
                tr(self.language, "nothing_playing").to_string(),
            )
        } else if self.audio.err.is_some() {
            (h.title.clone(), tr(self.language, "play_error").to_string())
        } else if self.audio.loading {
            (h.title.clone(), tr(self.language, "loading").to_string())
        } else {
            (h.title.clone(), h.artist.clone())
        };
        let tx = win.left() + 138.0;
        let tclip = Rect::from_min_max(pos2(tx, win.top() + 10.0), pos2(win.right() - 80.0, win.top() + 62.0));
        marquee(
            ui,
            &p,
            tclip,
            win.top() + 30.0,
            &title,
            FontId::proportional(14.0),
            TEXT,
        );
        marquee(
            ui,
            &p,
            tclip,
            win.top() + 49.0,
            &artist,
            FontId::proportional(11.0),
            MUTED,
        );

        let rb = Rect::from_center_size(pos2(win.right() - 44.0, win.top() + 18.0), vec2(24.0, 24.0));
        let rrs = ui.interact(rb, Id::new("mini_restore"), Sense::click());
        let rt = hov(ui, &rrs);
        if rt > 0.003 {
            p.rect_filled(rb.expand(1.5 * rt), 6.0, SURF2.gamma_multiply(rt));
        }
        let rcol = MUTED.lerp_to_gamma(TEXT, rt);
        p.rect_stroke(
            Rect::from_center_size(rb.center(), vec2(10.0, 10.0) * (1.0 + 0.2 * rt)),
            1.0,
            Stroke::new(1.3_f32, rcol),
        );
        hand(ui, &rrs);
        rrs.clone().on_hover_text(match self.language {
            Language::En => "Normal window",
            Language::Fr => "Fenêtre normale",
            Language::It => "Finestra normale",
            Language::Es => "Ventana normal",
            Language::Zh => "普通窗口",
            Language::Ru => "Обычное окно",
        });
        let cb = Rect::from_center_size(pos2(win.right() - 18.0, win.top() + 18.0), vec2(24.0, 24.0));
        let crs = ui.interact(cb, Id::new("mini_close"), Sense::click());
        let ct = hov(ui, &crs);
        if ct > 0.003 {
            p.rect_filled(cb.expand(1.5 * ct), 6.0, accent().gamma_multiply(ct));
        }
        let ccol = MUTED.lerp_to_gamma(on_accent(), ct);
        let cst = Stroke::new(1.3_f32, ccol);
        let cz = 1.0 + 0.2 * ct;
        p.line_segment(
            [cb.center() + vec2(-4.5, -4.5) * cz, cb.center() + vec2(4.5, 4.5) * cz],
            cst,
        );
        p.line_segment(
            [cb.center() + vec2(-4.5, 4.5) * cz, cb.center() + vec2(4.5, -4.5) * cz],
            cst,
        );
        hand(ui, &crs);
        if rrs.clicked() {
            self.exit_mini(&ctx);
            return;
        }
        if crs.clicked() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        let cy = win.top() + 86.0;
        let xc = (tx + win.right() - 12.0) / 2.0;
        let liked = self.is_liked();
        if ibtn(ui, Ic::Heart(liked), pos2(tx + 8.0, cy), 14.0, liked, "mn_like") {
            self.toggle_like();
        }
        if ibtn(ui, Ic::Prev, pos2(xc - 54.0, cy), 14.0, false, "mn_prev") {
            self.step(-1);
        }
        let pb = Rect::from_center_size(pos2(xc, cy), vec2(36.0, 36.0));
        let prs = ui.interact(pb, Id::new("mn_play"), Sense::click());
        let pz = pop(ui, &prs, pb, 3.0);
        p.rect_filled(pz, 8.0, accent());
        icon(
            &p,
            if self.playing { Ic::Pause } else { Ic::Play },
            pb.center(),
            13.0 * pz.width() / pb.width(),
            on_accent(),
        );
        hand(ui, &prs);
        if prs.clicked() {
            self.toggle_play();
        }
        if ibtn(ui, Ic::Next, pos2(xc + 54.0, cy), 14.0, false, "mn_next") {
            self.step(1);
        }

        let secs = if self.started { self.now_secs() } else { 0.0 };
        p.text(
            pos2(win.right() - 12.0, cy),
            Align2::RIGHT_CENTER,
            format!("{} / {}", mmss(self.progress * secs), mmss(secs)),
            FontId::proportional(10.0),
            MUTED,
        );
        let wr = Rect::from_min_max(
            pos2(tx, win.bottom() - 24.0),
            pos2(win.right() - 12.0, win.bottom() - 12.0),
        );
        if self.started {
            let (drag, commit) = wave_id(ui, wr, &mut self.progress, seed % 97, "mn_seek", secs);
            self.seeking = drag;
            if commit {
                self.audio.seek(self.progress * secs);
            }
        } else {
            wave_bars(&p, wr, 0.0, 0);
        }

        p.rect_stroke(win.shrink(0.5), (WIN_R - 0.5).max(0.0), Stroke::new(1.0_f32, LINE));
    }
}

fn avatar_default(p: &Painter, c: Pos2, r: f32, idx: usize) {
    let i = idx % GENRES.len();
    let pal = PAL[(i * 3 + 1) % PAL.len()];
    p.circle_filled(c, r, pal[0]);
    genre_icon(p, Rect::from_center_size(c, Vec2::splat(r * 1.1)), i, pal[1], pal[0]);
}

fn avatar_guest(p: &Painter, c: Pos2, r: f32) {
    p.circle_filled(c, r, SURF2);
    p.circle_filled(c + vec2(0.0, -0.22 * r), 0.27 * r, MUTED);
    let pts: Vec<Pos2> = (0..=16)
        .map(|k| {
            let a = std::f32::consts::PI * (1.0 + k as f32 / 16.0);
            c + vec2(a.cos() * 0.5 * r, 0.62 * r + a.sin() * 0.5 * r)
        })
        .collect();
    p.add(Shape::convex_polygon(pts, MUTED, Stroke::NONE));
}

fn avatar_photo(p: &Painter, c: Pos2, r: f32, tex: &TextureHandle) {
    let n = 48u32;
    let mut m = Mesh::with_texture(tex.id());
    m.vertices.push(egui::epaint::Vertex {
        pos: c,
        uv: pos2(0.5, 0.5),
        color: Color32::WHITE,
    });
    for k in 0..=n {
        let (s, co) = (TAU * k as f32 / n as f32).sin_cos();
        m.vertices.push(egui::epaint::Vertex {
            pos: c + vec2(co, s) * r,
            uv: pos2(0.5 + 0.5 * co, 0.5 + 0.5 * s),
            color: Color32::WHITE,
        });
    }
    for k in 0..n {
        m.add_triangle(0, 1 + k, 2 + k);
    }
    p.add(Shape::mesh(m));
}

fn draw_avatar(p: &Painter, c: Pos2, r: f32, av: Option<usize>, tex: Option<&TextureHandle>) {
    match (tex, av) {
        (Some(t), _) => avatar_photo(p, c, r, t),
        (None, Some(i)) => avatar_default(p, c, r, i),
        (None, None) => avatar_guest(p, c, r),
    }
}

fn top_c(ui: &mut Ui, w: f32, label: &str, title: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(w, 84.0), Sense::hover());
    let p = ui.painter();
    spaced(
        p,
        pos2(r.center().x, r.top() + 14.0),
        Align2::CENTER_CENTER,
        label,
        11.0,
        accent(),
    );
    p.text(
        pos2(r.center().x, r.top() + 52.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(32.0),
        TEXT,
    );
}

fn steps_bar(ui: &mut Ui, w: f32, cur: usize, n: usize) -> Option<usize> {
    let (r, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
    let (sw, gap) = (44.0_f32, 8.0_f32);
    let x0 = r.center().x - (n as f32 * sw + (n as f32 - 1.0) * gap) / 2.0;
    let mut clicked = None;
    for k in 0..n {
        let seg = Rect::from_min_size(pos2(x0 + k as f32 * (sw + gap), r.center().y - 2.0), vec2(sw, 4.0));
        let rs = ui.interact(seg.expand2(vec2(0.0, 9.0)), Id::new(("pf_step_seg", k)), Sense::click());
        let t = ui
            .ctx()
            .animate_bool_with_time(Id::new(("pf_seg_on", k)), k <= cur, 0.25);
        let p = ui.painter();
        p.rect_filled(seg, 2.0, LINE.lerp_to_gamma(MUTED, hov(ui, &rs) * 0.6));
        p.rect_filled(Rect::from_min_size(seg.min, vec2(sw * t, 4.0)), 2.0, accent());
        if k != cur {
            hand(ui, &rs);
        }
        if rs.clicked() {
            clicked = Some(k);
        }
    }
    clicked
}

fn pm_row(ui: &Ui, p: &Painter, rr: Rect, id: &str) -> Response {
    let rs = ui.interact(rr, Id::new(id), Sense::click());
    let t = hov(ui, &rs);
    if t > 0.003 {
        p.rect_filled(rr, 8.0, LINE.gamma_multiply(t));
    }
    hand(ui, &rs);
    rs
}
