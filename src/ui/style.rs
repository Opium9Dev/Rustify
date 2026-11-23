use super::super::*;

pub(in crate::app) fn fit(p: &Painter, pos: Pos2, text: &str, font: FontId, col: Color32, max_w: f32) {
    let width = |s: &str| p.layout_no_wrap(s.to_string(), font.clone(), col).size().x;
    let mut out = text.to_string();
    if width(&out) > max_w {
        let mut chars: Vec<char> = text.chars().collect();
        out = String::from("…");
        while !chars.is_empty() {
            chars.pop();
            let cand = chars.iter().collect::<String>().trim_end().to_string() + "…";
            if width(&cand) <= max_w {
                out = cand;
                break;
            }
        }
    }
    p.text(pos, Align2::LEFT_CENTER, out, font, col);
}

pub(in crate::app) fn setup_fonts(ctx: &Context) {
    let paths = [
        "C:/Windows/Fonts/malgun.ttf",
        "C:/Windows/Fonts/YuGothR.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/msjh.ttc",
        "C:/Windows/Fonts/segoeui.ttf",
        "C:/Windows/Fonts/leelawui.ttf",
        "C:/Windows/Fonts/Nirmala.ttc",
        "C:/Windows/Fonts/seguisym.ttf",
        "C:/Windows/Fonts/seguiemj.ttf",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    ];
    let mut fonts = FontDefinitions::default();
    for (i, path) in paths.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("fallback{i}");
            fonts.font_data.insert(name.clone(), FontData::from_owned(bytes));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                if let Some(list) = fonts.families.get_mut(&fam) {
                    list.push(name.clone());
                }
            }
        }
    }
    ctx.set_fonts(fonts);
}

pub(in crate::app) const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

pub(in crate::app) const BG: Color32 = c(12, 12, 14);

pub(in crate::app) const GLASS: Color32 = Color32::from_rgba_premultiplied(8, 8, 9, 160);
pub(in crate::app) const SURF: Color32 = Color32::from_rgba_premultiplied(13, 13, 16, 153);
pub(in crate::app) static GLASS_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(in crate::app) fn window_bg() -> Color32 {
    if GLASS_ON.load(AtOrd::Relaxed) {
        GLASS
    } else {
        BG
    }
}

pub(in crate::app) const SURF2: Color32 = Color32::from_rgba_premultiplied(34, 34, 40, 200);

pub(in crate::app) const LINE: Color32 = Color32::from_rgba_premultiplied(30, 30, 30, 30);

pub(in crate::app) const MENU: Color32 = Color32::from_rgba_premultiplied(35, 35, 42, 253);
pub(in crate::app) const HAIR: Color32 = Color32::from_rgba_premultiplied(22, 22, 22, 22);
pub(in crate::app) const TEXT: Color32 = c(242, 238, 230);
pub(in crate::app) const MUTED: Color32 = c(140, 138, 148);

pub(in crate::app) const ACCENT_DEFAULT: u32 = 0xFF5C34;
pub(in crate::app) static ACCENT_RGB: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(ACCENT_DEFAULT);

pub(in crate::app) const ACCENT_SWATCHES: [u32; 7] =
    [0xFF5C34, 0xFFA682, 0xFFA0BE, 0xB096FF, 0x82BEFF, 0x78D6C4, 0xF5D26E];

pub(in crate::app) fn rgb_of(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}
pub(in crate::app) fn pack_rgb(c: [u8; 3]) -> u32 {
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}
pub(in crate::app) fn accent() -> Color32 {
    rgb_of(ACCENT_RGB.load(AtOrd::Relaxed))
}

pub(in crate::app) fn on_accent() -> Color32 {
    readable(accent())
}
pub(in crate::app) fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let (r, g, b) = match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ]
}
pub(in crate::app) fn rgb_to_hsv(c: u32) -> [f32; 3] {
    let (r, g, b) = (
        ((c >> 16) & 255) as f32 / 255.0,
        ((c >> 8) & 255) as f32 / 255.0,
        (c & 255) as f32 / 255.0,
    );
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let d = mx - mn;
    let h = if d <= 0.0 {
        0.0
    } else if mx == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if mx == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, if mx <= 0.0 { 0.0 } else { d / mx }, mx]
}

pub(in crate::app) fn pipette_icon(p: &Painter, c: Pos2, col: Color32) {
    p.line_segment([c + vec2(-6.0, 6.0), c + vec2(-3.0, 3.0)], Stroke::new(1.6_f32, col));
    p.line_segment([c + vec2(-3.0, 3.0), c + vec2(2.0, -2.0)], Stroke::new(4.0_f32, col));
    p.line_segment([c + vec2(-1.0, -4.0), c + vec2(4.0, 1.0)], Stroke::new(1.6_f32, col));
    p.circle_filled(c + vec2(4.5, -4.5), 2.6, col);
}

pub(in crate::app) const PAL: [[Color32; 2]; 8] = [
    [c(255, 92, 52), c(18, 18, 20)],
    [c(238, 230, 216), c(255, 92, 52)],
    [c(66, 76, 255), c(238, 230, 216)],
    [c(255, 198, 46), c(18, 18, 20)],
    [c(255, 160, 190), c(70, 20, 44)],
    [c(36, 38, 46), c(255, 92, 52)],
    [c(120, 214, 196), c(10, 44, 52)],
    [c(168, 140, 255), c(30, 18, 70)],
];
