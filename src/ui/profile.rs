use super::super::*;

impl App {
    pub(in crate::app) fn cur_profile(&self) -> Option<Profile> {
        self.cur.and_then(|i| self.profiles.get(i)).cloned()
    }

    pub(in crate::app) fn photo_tex(&mut self, ctx: &Context, p: &Profile) -> Option<TextureHandle> {
        if !p.photo {
            return None;
        }
        if let Some(t) = self.photos.get(&p.id) {
            return t.clone();
        }
        let tex = std::fs::read(profiles_dir().join(&p.id).join("avatar.png"))
            .ok()
            .and_then(|b| ::image::load_from_memory(&b).ok())
            .map(|img| {
                let img = img.to_rgba8();
                let ci =
                    ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                ctx.load_texture(format!("pf_{}", p.id), ci, TextureOptions::LINEAR)
            });
        self.photos.insert(p.id.clone(), tex.clone());
        tex
    }

    pub(in crate::app) fn refresh_seed(&mut self) {
        let art = self.history.first().map(|h| h.artist.clone()).filter(|a| !a.is_empty());
        let tastes = self
            .cur
            .and_then(|i| self.profiles.get(i))
            .map(|p| p.tastes.clone())
            .unwrap_or_default();
        self.fy_taste = None;
        self.fy_seed = match art {
            Some(a) => a,
            None if !tastes.is_empty() => {
                let t = tastes[rand_below(tastes.len())];
                self.fy_taste = Some(t);
                taste_query(t)
            }
            None => "new music hits 2026".to_string(),
        };
    }

    pub(in crate::app) fn taste_rows(&self) -> Vec<usize> {
        if self.mood != 0 {
            return Vec::new();
        }
        self.cur
            .and_then(|i| self.profiles.get(i))
            .map(|p| {
                p.tastes
                    .iter()
                    .copied()
                    .filter(|t| Some(*t) != self.fy_taste)
                    .take(2)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(in crate::app) fn clear_user(&mut self) {
        self.audio.stop();
        self.playing = false;
        self.started = false;
        self.last_playing = false;
        self.queue.clear();
        self.qi = 0;
        self.progress = 0.0;
        self.sel = None;
        self.shuf_next = None;
        self.likes.clear();
        self.liked_ids.clear();
        self.history.clear();
        self.recent.clear();
        self.playlists.clear();
        self.open_pl = None;
        self.pl_edit = None;
        self.pl_del = None;
        self.pl_menu = None;
        self.pl_ctx = None;
        self.query.clear();
        self.seen_query.clear();
        self.sent_query.clear();
        self.hits.clear();
        self.searching = false;
        self.search_err = None;
        self.rx = None;
        self.mood = 0;
        self.tab = 0;
        self.view = View::Home;
        self.all_for_you = false;
        self.hero_bg = None;
        self.lang_open = false;
    }

    pub(in crate::app) fn switch_to(&mut self, who: Option<usize>) {
        self.clear_user();
        match who.and_then(|i| self.profiles.get(i)) {
            Some(p) => {
                set_user(&p.id);
                ACCENT_RGB.store(p.accent.unwrap_or(ACCENT_DEFAULT), AtOrd::Relaxed);
                self.language = Language::from_code(&p.language);
            }
            None => {
                set_user("");
                wipe_guest();
                ACCENT_RGB.store(ACCENT_DEFAULT, AtOrd::Relaxed);
                self.language = load_language();
            }
        }
        self.cur = who.filter(|i| *i < self.profiles.len());
        self.likes = load_likes();
        self.liked_ids = self.likes.iter().map(|h| h.id.clone()).collect();
        self.history = load_history();
        self.recent = load_recent();
        self.playlists = load_playlists();
        self.refresh_seed();
        self.greet_seed = rand_below(10_000);
        self.entered = true;
        self.form = None;
        self.pui = Pui::Off;
    }

    pub(in crate::app) fn open_form(&mut self, edit: Option<usize>) {
        let mut f = Form {
            edit,
            name: String::new(),
            tastes: [false; 12],
            avatar: None,
            photo: None,
            keep_photo: false,
            del: false,
            focus: true,
            step: 0,
            goto: 0,
            dir: 1.0,
            phase: 0,
            t0: 0.0,
            caret: Caret::default(),
            err: None,
        };
        if let Some(p) = edit.and_then(|i| self.profiles.get(i)) {
            f.name = p.name.clone();
            for &t in &p.tastes {
                f.tastes[t] = true;
            }
            f.keep_photo = p.photo;
            if !p.photo {
                f.avatar = Some(p.avatar);
            }
        }
        self.form = Some(f);
        self.pui = Pui::Form;
    }

    pub(in crate::app) fn commit_form(&mut self, f: &Form, now: f64) {
        let name = f.name.trim().to_string();
        let tastes: Vec<usize> = (0..GENRES.len()).filter(|&i| f.tastes[i]).collect();
        match f.edit.filter(|i| *i < self.profiles.len()) {
            Some(i) => {
                let id = self.profiles[i].id.clone();
                let dir = profiles_dir().join(&id);
                let _ = std::fs::create_dir_all(&dir);
                let photo = if let Some((img, _)) = &f.photo {
                    img.save(dir.join("avatar.png")).is_ok()
                } else if f.keep_photo {
                    true
                } else {
                    let _ = std::fs::remove_file(dir.join("avatar.png"));
                    false
                };
                let pr = &mut self.profiles[i];
                pr.name = name;
                pr.tastes = tastes;
                pr.photo = photo;
                pr.avatar = f.avatar.unwrap_or(pr.avatar);
                pr.language = self.language.code().to_string();
                self.photos.remove(&id);
                save_profiles(&self.profiles);
                self.refresh_seed();
                self.form = None;
                self.pui = Pui::Off;
            }
            None => {
                let id = new_profile_id();
                let dir = profiles_dir().join(&id);
                let _ = std::fs::create_dir_all(&dir);
                if self.profiles.is_empty() {
                    migrate_legacy(&dir);
                }
                let photo = f
                    .photo
                    .as_ref()
                    .map(|(img, _)| img.save(dir.join("avatar.png")).is_ok())
                    .unwrap_or(false);
                let avatar = f.avatar.unwrap_or_else(|| seed_of(&name) % GENRES.len());
                self.profiles.push(Profile {
                    id,
                    name,
                    tastes,
                    avatar,
                    photo,
                    accent: None,
                    language: self.language.code().to_string(),
                });
                save_profiles(&self.profiles);
                let n = self.profiles.len() - 1;
                self.request_switch(now, Some(n));
            }
        }
    }

    pub(in crate::app) fn delete_profile(&mut self, i: usize) {
        if i >= self.profiles.len() {
            return;
        }
        let p = self.profiles.remove(i);
        let _ = std::fs::remove_dir_all(profiles_dir().join(&p.id));
        self.photos.remove(&p.id);
        save_profiles(&self.profiles);
        self.clear_user();
        set_user("");
        wipe_guest();
        self.cur = None;
        self.entered = false;
        self.form = None;
        self.pui = Pui::Picker;
    }

    pub(in crate::app) fn profile_screen(&mut self, ui: &mut Ui, full: Rect) {
        match self.pui {
            Pui::Picker => self.picker(ui, full),
            Pui::Form => self.form_screen(ui, full),
            _ => {}
        }
    }

    pub(in crate::app) fn picker(&mut self, ui: &mut Ui, full: Rect) {
        enum Pick {
            Prof(usize),
            Guest,
            New,
        }
        let ctx = ui.ctx().clone();
        let profs = self.profiles.clone();
        let texs: Vec<Option<TextureHandle>> = profs.iter().map(|p| self.photo_tex(&ctx, p)).collect();
        let n = profs.len() + 2;
        let (tw, th, gap) = (132.0_f32, 168.0_f32, 24.0_f32);
        let colw = (full.width() - 48.0).min(920.0);
        let per = (((colw + gap) / (tw + gap)) as usize).clamp(1, 6);
        let rows = (n + per - 1) / per;
        let content_h = 84.0 + 28.0 + rows as f32 * (th + gap);
        let top_pad = ((full.height() - content_h) / 2.0 - 10.0).max(12.0);
        let col = Rect::from_min_max(
            pos2(full.center().x - colw / 2.0, full.top()),
            pos2(full.center().x + colw / 2.0, full.bottom()),
        );
        let mut pick: Option<Pick> = None;
        ui.allocate_new_ui(UiBuilder::new().max_rect(col), |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.add_space(top_pad);
                    top_c(
                        ui,
                        colw,
                        tr(self.language, "welcome"),
                        tr(self.language, "choose_profile"),
                    );
                    ui.add_space(28.0);
                    let (gr, _) = ui.allocate_exact_size(vec2(colw, rows as f32 * (th + gap)), Sense::hover());
                    for idx in 0..n {
                        let row = idx / per;
                        let m = per.min(n - row * per);
                        let row_w = m as f32 * tw + (m as f32 - 1.0) * gap;
                        let x0 = (colw - row_w) / 2.0 + (idx % per) as f32 * (tw + gap);
                        let r = Rect::from_min_size(gr.min + vec2(x0, row as f32 * (th + gap)), vec2(tw, th));
                        let rs = ui.interact(r, Id::new(("pk", idx)), Sense::click());
                        let t = hov(ui, &rs);
                        let c = pos2(r.center().x, r.top() + 58.0);
                        let p = ui.painter().clone();
                        let (label, sub): (String, String) = if idx < profs.len() {
                            let pr = &profs[idx];
                            let g: Vec<&str> = pr
                                .tastes
                                .iter()
                                .take(2)
                                .map(|&k| genre_name(self.language, k))
                                .collect();
                            (pr.name.clone(), g.join(" · "))
                        } else if idx == profs.len() {
                            (
                                tr(self.language, "guest").to_string(),
                                tr(self.language, "no_profile").to_string(),
                            )
                        } else {
                            (tr(self.language, "new_profile").to_string(), String::new())
                        };
                        let rad = 48.0 + 3.0 * t;
                        if idx < profs.len() {
                            draw_avatar(&p, c, rad, Some(profs[idx].avatar), texs[idx].as_ref());
                        } else if idx == profs.len() {
                            avatar_guest(&p, c, rad);
                        } else {
                            p.circle_filled(c, rad, SURF);
                            p.circle_stroke(c, rad, Stroke::new(1.0_f32, LINE.lerp_to_gamma(MUTED, t)));
                            let st = Stroke::new(2.4_f32, accent());
                            p.line_segment([c + vec2(-11.0, 0.0), c + vec2(11.0, 0.0)], st);
                            p.line_segment([c + vec2(0.0, -11.0), c + vec2(0.0, 11.0)], st);
                        }
                        if t > 0.003 {
                            p.circle_stroke(c, 53.0 + 3.0 * t, Stroke::new(2.0_f32, accent().gamma_multiply(t)));
                        }
                        p.text(
                            pos2(r.center().x, r.top() + 132.0),
                            Align2::CENTER_CENTER,
                            short(&label, 14),
                            FontId::proportional(15.0),
                            MUTED.lerp_to_gamma(TEXT, t),
                        );
                        if !sub.is_empty() {
                            p.text(
                                pos2(r.center().x, r.top() + 151.0),
                                Align2::CENTER_CENTER,
                                short(&sub, 20),
                                FontId::proportional(11.0),
                                MUTED,
                            );
                        }
                        hand(ui, &rs);
                        if rs.clicked() {
                            pick = Some(if idx < profs.len() {
                                Pick::Prof(idx)
                            } else if idx == profs.len() {
                                Pick::Guest
                            } else {
                                Pick::New
                            });
                        }
                    }
                });
        });
        let now = ctx.input(|i| i.time);
        match pick {
            Some(Pick::Prof(i)) => self.request_switch(now, Some(i)),
            Some(Pick::Guest) => self.request_switch(now, None),
            Some(Pick::New) => self.open_form(None),
            None => {}
        }
    }

    pub(in crate::app) fn form_screen(&mut self, ui: &mut Ui, full: Rect) {
        let Some(mut f) = self.form.take() else {
            self.pui = if self.entered { Pui::Off } else { Pui::Picker };
            return;
        };
        let ctx = ui.ctx().clone();

        let dropped = ui.input(|i| i.raw.dropped_files.clone());
        let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
        for d in dropped {
            let bytes = match (&d.bytes, &d.path) {
                (Some(b), _) => Some(b.to_vec()),
                (None, Some(p)) => std::fs::read(p).ok(),
                _ => None,
            };
            match bytes.and_then(|b| ::image::load_from_memory(&b).ok()) {
                Some(img) => {
                    let sq = img
                        .resize_to_fill(PHOTO_PX, PHOTO_PX, ::image::imageops::FilterType::Lanczos3)
                        .to_rgba8();
                    let ci = ColorImage::from_rgba_unmultiplied([PHOTO_PX as usize; 2], sq.as_raw());
                    let tex = ctx.load_texture("pf_photo_new", ci, TextureOptions::LINEAR);
                    f.photo = Some((sq, tex));
                    f.avatar = None;
                    f.keep_photo = false;
                    f.err = None;
                    f.step = 1;
                }
                None => f.err = Some(tr(self.language, "image_error")),
            }
        }

        let cur_prof = f.edit.and_then(|i| self.profiles.get(i)).cloned();
        let cur_tex = if f.keep_photo {
            cur_prof.as_ref().and_then(|p| self.photo_tex(&ctx, p))
        } else {
            None
        };
        let ptex: Option<TextureHandle> = f.photo.as_ref().map(|(_, t)| t.clone()).or(cur_tex);
        let editing = f.edit.is_some();

        let now = ui.input(|i| i.time);
        match f.phase {
            1 => {
                if (now - f.t0) / 0.14 >= 1.0 {
                    f.step = f.goto;
                    f.phase = 2;
                    f.t0 = now;
                    f.focus = f.step == 0;
                }
                ctx.request_repaint();
            }
            2 => {
                if now - f.t0 >= 0.2 {
                    f.phase = 0;
                }
                ctx.request_repaint();
            }
            _ => {}
        }
        let prog = match f.phase {
            1 => 1.0 - (((now - f.t0) / 0.14) as f32).min(1.0),
            2 => (((now - f.t0) / 0.2) as f32).min(1.0),
            _ => 1.0,
        };
        let k = prog * prog * (3.0 - 2.0 * prog);
        let off = (1.0 - k) * 36.0 * if f.phase == 1 { -f.dir } else { f.dir };
        let busy = f.phase != 0;

        let fid = FontId::proportional(14.0);
        let widths: Vec<f32> = (0..GENRES.len())
            .map(|i| {
                ui.painter()
                    .layout_no_wrap(genre_name(self.language, i).to_string(), fid.clone(), TEXT)
                    .size()
                    .x
                    + 30.0
            })
            .collect();
        let colw = (full.width() - 48.0).min(560.0);
        let mut rows_idx: Vec<Vec<usize>> = vec![Vec::new()];
        let mut xx = 0.0_f32;
        for (i, wd) in widths.iter().enumerate() {
            if xx + wd > colw && xx > 0.0 {
                rows_idx.push(Vec::new());
                xx = 0.0;
            }
            rows_idx.last_mut().unwrap().push(i);
            xx += wd + 8.0;
        }
        let body_h = match f.step {
            0 => 78.0,
            1 => 250.0,
            _ => rows_idx.len() as f32 * 42.0 + 36.0,
        };
        let total_h = 84.0 + 28.0 + 24.0 + body_h + 30.0 + 44.0;
        let top_pad = ((full.height() - total_h) / 2.0 - 8.0).max(12.0);
        let col = Rect::from_min_max(
            pos2(full.center().x - colw / 2.0 + off, full.top()),
            pos2(full.center().x + colw / 2.0 + off, full.bottom()),
        );

        let name_ok = !f.name.trim().is_empty();
        let mut act: Option<FormAct> = None;
        let mut go: Option<usize> = None;

        ui.allocate_new_ui(UiBuilder::new().max_rect(col), |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    let w = colw;
                    let step = f.step;
                    ui.add_space(top_pad);
                    let titles = [
                        match self.language {
                            Language::En => "What's your name?",
                            Language::Fr => "C'est quoi ton prénom ?",
                            Language::It => "Come ti chiami?",
                            Language::Es => "¿Cómo te llamas?",
                            Language::Zh => "你的名字是？",
                            Language::Ru => "Как вас зовут?",
                        },
                        match self.language {
                            Language::En => "Choose a photo?",
                            Language::Fr => "Une photo pour toi ?",
                            Language::It => "Una foto per te?",
                            Language::Es => "¿Una foto para ti?",
                            Language::Zh => "选择一张照片？",
                            Language::Ru => "Добавить фото?",
                        },
                        match self.language {
                            Language::En => "What do you listen to?",
                            Language::Fr => "Qu'est-ce que tu écoutes ?",
                            Language::It => "Cosa ascolti?",
                            Language::Es => "¿Qué escuchas?",
                            Language::Zh => "你喜欢听什么？",
                            Language::Ru => "Что вы слушаете?",
                        },
                    ];
                    let label = match self.language {
                        Language::En => format!(
                            "{} · STEP {} OF 3",
                            if editing { "EDIT" } else { "NEW PROFILE" },
                            step + 1
                        ),
                        Language::Fr => format!(
                            "{} · ÉTAPE {} SUR 3",
                            if editing { "MODIFIER" } else { "NOUVEAU PROFIL" },
                            step + 1
                        ),
                        Language::It => format!(
                            "{} · PASSO {} DI 3",
                            if editing { "MODIFICA" } else { "NUOVO PROFILO" },
                            step + 1
                        ),
                        Language::Es => format!(
                            "{} · PASO {} DE 3",
                            if editing { "EDITAR" } else { "NUEVO PERFIL" },
                            step + 1
                        ),
                        Language::Zh => format!(
                            "{} · 第 {} 步，共 3 步",
                            if editing { "编辑" } else { "新建个人资料" },
                            step + 1
                        ),
                        Language::Ru => format!(
                            "{} · ШАГ {} ИЗ 3",
                            if editing {
                                "ИЗМЕНИТЬ"
                            } else {
                                "НОВЫЙ ПРОФИЛЬ"
                            },
                            step + 1
                        ),
                    };
                    top_c(ui, w, &label, titles[step.min(2)]);
                    if let Some(s) = steps_bar(ui, w, step, 3) {
                        if editing || s < step {
                            go = Some(s);
                        }
                    }
                    ui.add_space(24.0);

                    match step {
                        0 => {
                            let (nr0, _) = ui.allocate_exact_size(vec2(w, 48.0), Sense::hover());
                            let nr = Rect::from_center_size(nr0.center(), vec2(w.min(420.0), 48.0));
                            let nid = Id::new("pf_name");
                            let nrs = ui.interact(nr, Id::new("pf_name_bar"), Sense::click());
                            if nrs.clicked() {
                                ui.memory_mut(|m| m.request_focus(nid));
                            }
                            if f.focus {
                                ui.memory_mut(|m| m.request_focus(nid));
                                f.focus = false;
                            }
                            let focused = ui.memory(|m| m.has_focus(nid));
                            ui.painter().rect_filled(nr, 8.0, SURF);
                            let bh = hov_if(ui, Id::new("pf_name_hov"), ui.rect_contains_pointer(nr));
                            let bf = hov_if(ui, Id::new("pf_name_focus"), focused);
                            ui.painter().rect_stroke(
                                nr,
                                8.0,
                                Stroke::new(1.0_f32, LINE.gamma_multiply(bh * (1.0 - bf))),
                            );
                            ui.painter()
                                .rect_stroke(nr, 8.0, Stroke::new(1.0_f32, accent().gamma_multiply(bf)));
                            text_field(
                                ui,
                                Rect::from_min_max(nr.min + vec2(18.0, 0.0), nr.max - vec2(18.0, 0.0)),
                                nid,
                                &mut f.name,
                                tr(self.language, "name_placeholder"),
                                &mut f.caret,
                            );
                            if f.name.chars().count() > 20 {
                                f.name = f.name.chars().take(20).collect();
                            }
                            if focused && !f.name.trim().is_empty() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                go = Some(1);
                            }
                            let (hr, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
                            ui.painter().text(
                                hr.center() + vec2(0.0, 4.0),
                                Align2::CENTER_CENTER,
                                tr(self.language, "name_hint"),
                                FontId::proportional(12.0),
                                MUTED,
                            );
                        }

                        1 => {
                            let (ar, _) = ui.allocate_exact_size(vec2(w, 250.0), Sense::hover());
                            let p = ui.painter().clone();
                            let cx = ar.center().x;
                            let auto_av = f.avatar.unwrap_or_else(|| {
                                cur_prof
                                    .as_ref()
                                    .map(|c| c.avatar)
                                    .unwrap_or_else(|| seed_of(f.name.trim()) % GENRES.len())
                            });
                            let pc = pos2(cx, ar.top() + 52.0);
                            draw_avatar(&p, pc, 46.0, Some(auto_av), ptex.as_ref());
                            if hovering {
                                p.circle_stroke(pc, 53.0, Stroke::new(2.0_f32, accent()));
                            }
                            for i in 0..GENRES.len() {
                                let c = pos2(
                                    cx - 125.0 + (i % 6) as f32 * 50.0,
                                    ar.top() + 136.0 + (i / 6) as f32 * 50.0,
                                );
                                let rs = ui.interact(
                                    Rect::from_center_size(c, Vec2::splat(44.0)),
                                    Id::new(("pf_av", i)),
                                    Sense::click(),
                                );
                                let t = hov(ui, &rs);
                                let sel = f.photo.is_none() && !f.keep_photo && f.avatar == Some(i);
                                draw_avatar(&p, c, 20.0 + 2.0 * t, Some(i), None);
                                if sel {
                                    p.circle_stroke(c, 25.0, Stroke::new(2.0_f32, accent()));
                                }
                                hand(ui, &rs);
                                if rs.clicked() {
                                    f.avatar = Some(i);
                                    f.photo = None;
                                    f.keep_photo = false;
                                    f.err = None;
                                }
                            }
                            let (hint, hc) = match f.err {
                                Some(e) => (e, accent()),
                                None => (tr(self.language, "avatar_hint"), MUTED),
                            };
                            p.text(
                                pos2(cx, ar.top() + 238.0),
                                Align2::CENTER_CENTER,
                                hint,
                                FontId::proportional(12.0),
                                hc,
                            );
                        }

                        _ => {
                            let (cr, _) = ui.allocate_exact_size(vec2(w, rows_idx.len() as f32 * 42.0), Sense::hover());
                            for (ri, row) in rows_idx.iter().enumerate() {
                                let rw: f32 =
                                    row.iter().map(|&i| widths[i]).sum::<f32>() + 8.0 * (row.len() as f32 - 1.0);
                                let mut x = (w - rw) / 2.0;
                                for &i in row {
                                    let br =
                                        Rect::from_min_size(cr.min + vec2(x, ri as f32 * 42.0), vec2(widths[i], 34.0));
                                    x += widths[i] + 8.0;
                                    let rs = ui.interact(br, Id::new(("pf_taste", i)), Sense::click());
                                    let on = f.tastes[i];
                                    let t = hov(ui, &rs);
                                    let bz = pop(ui, &rs, br, 1.5);
                                    let pal = PAL[(i * 3 + 1) % PAL.len()];
                                    let p = ui.painter();
                                    if on {
                                        p.rect_filled(bz, 8.0, pal[0]);
                                    } else {
                                        p.rect_filled(bz, 8.0, SURF.gamma_multiply(t));
                                        p.rect_stroke(bz, 8.0, Stroke::new(1.0_f32, LINE.lerp_to_gamma(MUTED, t)));
                                    }
                                    p.text(
                                        br.center(),
                                        Align2::CENTER_CENTER,
                                        genre_name(self.language, i),
                                        fid.clone(),
                                        if on { pal[1] } else { TEXT },
                                    );
                                    hand(ui, &rs);
                                    if rs.clicked() {
                                        f.tastes[i] = !on;
                                    }
                                }
                            }
                            let nsel = f.tastes.iter().filter(|b| **b).count();
                            let msg = if nsel == 0 {
                                match self.language {
                                    Language::En => "Choose at least one genre to personalize recommendations.",
                                    Language::Fr => "Choisis au moins un genre pour orienter tes recommandations.",
                                    Language::It => "Scegli almeno un genere per personalizzare i consigli.",
                                    Language::Es => "Elige al menos un género para personalizar las recomendaciones.",
                                    Language::Zh => "请至少选择一种音乐类型来调整推荐。",
                                    Language::Ru => "Выберите хотя бы один жанр для персональных рекомендаций.",
                                }
                                .to_string()
                            } else {
                                match self.language {
                                    Language::En => format!("{nsel} genre(s) selected"),
                                    Language::Fr => format!(
                                        "{nsel} genre{} choisi{}",
                                        if nsel > 1 { "s" } else { "" },
                                        if nsel > 1 { "s" } else { "" }
                                    ),
                                    Language::It => format!("{nsel} generi selezionati"),
                                    Language::Es => format!("{nsel} géneros seleccionados"),
                                    Language::Zh => format!("已选择 {nsel} 种类型"),
                                    Language::Ru => format!("Выбрано жанров: {nsel}"),
                                }
                            };
                            let (hr, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
                            ui.painter().text(
                                hr.center() + vec2(0.0, 6.0),
                                Align2::CENTER_CENTER,
                                msg,
                                FontId::proportional(12.0),
                                MUTED,
                            );
                        }
                    }

                    ui.add_space(30.0);
                    let (br0, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
                    let nw = w.min(460.0);
                    let br = Rect::from_center_size(br0.center(), vec2(nw, 44.0));
                    let left_lab = if step == 0 {
                        tr(self.language, "cancel")
                    } else {
                        tr(self.language, "back")
                    };
                    if Self::text_btn(
                        ui,
                        Rect::from_min_size(br.min, vec2(110.0, 40.0)),
                        left_lab,
                        "pf_back",
                        false,
                    ) && !busy
                    {
                        if step == 0 {
                            act = Some(FormAct::Cancel);
                        } else {
                            go = Some(step - 1);
                        }
                    }
                    if editing && step == 0 {
                        let (lab, dw) = if f.del {
                            (tr(self.language, "confirm"), 120.0)
                        } else {
                            (tr(self.language, "delete"), 110.0)
                        };
                        if Self::text_btn(
                            ui,
                            Rect::from_min_size(br.min + vec2(122.0, 0.0), vec2(dw, 40.0)),
                            lab,
                            "pf_del",
                            f.del,
                        ) {
                            if f.del {
                                act = Some(FormAct::Delete);
                            } else {
                                f.del = true;
                            }
                        }
                    }
                    let valid = match step {
                        0 => name_ok,
                        1 => true,
                        _ => name_ok && f.tastes.iter().any(|b| *b),
                    };
                    let lab = if step < 2 {
                        tr(self.language, "continue")
                    } else if editing {
                        tr(self.language, "save")
                    } else {
                        tr(self.language, "create_profile")
                    };
                    let pr = Rect::from_min_size(pos2(br.right() - 170.0, br.top()), vec2(170.0, 40.0));
                    let prs = ui.interact(pr, Id::new("pf_next"), Sense::click());
                    let pz = if valid { pop(ui, &prs, pr, 2.0) } else { pr };
                    ui.painter().rect_filled(pz, 8.0, if valid { accent() } else { SURF2 });
                    ui.painter().text(
                        pr.center(),
                        Align2::CENTER_CENTER,
                        lab,
                        FontId::proportional(14.0),
                        if valid { on_accent() } else { MUTED },
                    );
                    if valid {
                        hand(ui, &prs);
                        if prs.clicked() && !busy {
                            if step < 2 {
                                go = Some(step + 1);
                            } else {
                                act = Some(FormAct::Save);
                            }
                        }
                    }
                    ui.add_space(40.0);

                    if k < 0.999 {
                        let vr = ui.clip_rect();
                        ui.painter().rect_filled(vr, 0.0, BG.gamma_multiply(1.0 - k));
                    }
                });
        });

        if let Some(s) = go {
            if !busy && s != f.step {
                f.dir = if s > f.step { 1.0 } else { -1.0 };
                f.goto = s;
                f.phase = 1;
                f.t0 = now;
                ctx.request_repaint();
            }
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            act = Some(FormAct::Cancel);
        }
        if self.sw.is_some() {
            act = None;
        }
        match act {
            Some(FormAct::Cancel) => self.pui = if self.entered { Pui::Off } else { Pui::Picker },
            Some(FormAct::Save) => {
                self.commit_form(&f, now);

                if self.sw.is_some() {
                    self.form = Some(f);
                }
            }
            Some(FormAct::Delete) => {
                if let Some(i) = f.edit {
                    self.delete_profile(i);
                }
            }
            None => self.form = Some(f),
        }
    }

    pub(in crate::app) fn set_accent(&mut self, rgb: u32) {
        ACCENT_RGB.store(rgb, AtOrd::Relaxed);
        if let Some(i) = self.cur.filter(|i| *i < self.profiles.len()) {
            self.profiles[i].accent = Some(rgb);
            self.acc_dirty = true;
        }
    }

    pub(in crate::app) fn accent_picker(&mut self, ui: &mut Ui, r: Rect) {
        ui.interact(r, Id::new("acc_pick_block"), Sense::click());
        let p = ui.painter().clone();
        acrylic_panel(&p, r, 12.0);
        p.rect_stroke(r.shrink(0.5), 12.0, Stroke::new(1.0_f32, LINE));
        let sv = Rect::from_min_size(r.min + vec2(12.0, 12.0), vec2(r.width() - 24.0, 124.0));
        let hb = Rect::from_min_size(pos2(sv.left(), sv.bottom() + 12.0), vec2(sv.width(), 14.0));
        let [mut h, mut s, mut v] = self.acc_hsv;
        let mut changed = false;
        let svr = ui.interact(sv, Id::new("acc_sv"), Sense::click_and_drag());
        if svr.dragged() || svr.clicked() {
            if let Some(pt) = svr.interact_pointer_pos() {
                s = ((pt.x - sv.left()) / sv.width()).clamp(0.0, 1.0);
                v = (1.0 - (pt.y - sv.top()) / sv.height()).clamp(0.0, 1.0);
                changed = true;
            }
        }
        let hr = ui.interact(hb.expand2(vec2(0.0, 6.0)), Id::new("acc_hue"), Sense::click_and_drag());
        if hr.dragged() || hr.clicked() {
            if let Some(pt) = hr.interact_pointer_pos() {
                h = ((pt.x - hb.left()) / hb.width()).clamp(0.0, 0.999);
                changed = true;
            }
        }
        hand(ui, &svr);
        hand(ui, &hr);

        let hue = rgb_of(pack_rgb(hsv_to_rgb(h, 1.0, 1.0)));
        let mut m = Mesh::default();
        m.colored_vertex(sv.left_top(), Color32::WHITE);
        m.colored_vertex(sv.right_top(), hue);
        m.colored_vertex(sv.right_bottom(), hue);
        m.colored_vertex(sv.left_bottom(), Color32::WHITE);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        p.add(Shape::mesh(m));
        let mut m = Mesh::default();
        m.colored_vertex(sv.left_top(), Color32::TRANSPARENT);
        m.colored_vertex(sv.right_top(), Color32::TRANSPARENT);
        m.colored_vertex(sv.right_bottom(), Color32::BLACK);
        m.colored_vertex(sv.left_bottom(), Color32::BLACK);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        p.add(Shape::mesh(m));

        let mut m = Mesh::default();
        for k in 0..=6u32 {
            let x = hb.left() + hb.width() * k as f32 / 6.0;
            let c = rgb_of(pack_rgb(hsv_to_rgb(k as f32 / 6.0, 1.0, 1.0)));
            m.colored_vertex(pos2(x, hb.top()), c);
            m.colored_vertex(pos2(x, hb.bottom()), c);
        }
        for k in 0..6u32 {
            let a = 2 * k;
            m.add_triangle(a, a + 1, a + 2);
            m.add_triangle(a + 1, a + 3, a + 2);
        }
        p.add(Shape::mesh(m));

        let tp = pos2(sv.left() + s * sv.width(), sv.top() + (1.0 - v) * sv.height());
        p.circle_stroke(tp, 7.0, Stroke::new(1.0_f32, Color32::from_black_alpha(110)));
        p.circle_stroke(tp, 6.0, Stroke::new(2.0_f32, Color32::WHITE));
        let hx = hb.left() + h * hb.width();
        p.rect_stroke(
            Rect::from_center_size(pos2(hx, hb.center().y), vec2(6.0, hb.height() + 4.0)),
            3.0,
            Stroke::new(2.0_f32, Color32::WHITE),
        );

        let rgb = hsv_to_rgb(h, s, v);
        let pv = Rect::from_min_size(pos2(sv.left(), hb.bottom() + 12.0), vec2(26.0, 26.0));
        p.rect_filled(pv, 8.0, rgb_of(pack_rgb(rgb)));
        p.text(
            pos2(pv.right() + 10.0, pv.center().y),
            Align2::LEFT_CENTER,
            format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]),
            FontId::proportional(13.0),
            MUTED,
        );

        if changed {
            self.acc_hsv = [h, s, v];
            self.set_accent(pack_rgb(rgb));
        }
    }

    pub(in crate::app) fn profile_panel(&mut self, ui: &mut Ui, full: Rect, rail: Rect) {
        if self.acc_dirty && !ui.input(|i| i.pointer.any_down()) {
            save_profiles(&self.profiles);
            self.acc_dirty = false;
        }
        if self.pui != Pui::Panel {
            self.acc_pick = false;
            self.pm_del = false;
            return;
        }
        enum PAct {
            Edit,
            Switch(Option<usize>),
            New,
            Delete,
        }
        let bd = ui.interact(full, Id::new("pf_backdrop"), Sense::click());
        if bd.clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
            self.pui = Pui::Off;
            return;
        }
        let ctx = ui.ctx().clone();
        let me = self.cur_profile();
        let others: Vec<(usize, Profile)> = self
            .profiles
            .iter()
            .cloned()
            .enumerate()
            .filter(|(i, _)| Some(*i) != self.cur)
            .take(5)
            .collect();
        let n_sw = others.len() + me.is_some() as usize;
        let mw = 300.0_f32;
        let mh = 112.0
            + if me.is_some() { 44.0 } else { 0.0 }
            + 78.0
            + 44.0
            + 44.0
            + 34.0
            + if n_sw > 0 { 34.0 + n_sw as f32 * 46.0 } else { 0.0 }
            + 44.0
            + if me.is_some() { 44.0 } else { 0.0 }
            + 10.0;
        let r = Rect::from_min_size(
            pos2(rail.right() + 10.0, (rail.bottom() - mh).max(full.top() + 8.0)),
            vec2(mw, mh),
        );
        ui.interact(r, Id::new("pf_panel_block"), Sense::click());
        let p = ui.painter().clone();
        acrylic_panel(&p, r, 12.0);
        p.rect_stroke(r.shrink(0.5), 12.0, Stroke::new(1.0_f32, LINE));

        let tex = me.as_ref().and_then(|pr| self.photo_tex(&ctx, pr));
        draw_avatar(
            &p,
            pos2(r.left() + 44.0, r.top() + 44.0),
            28.0,
            me.as_ref().map(|pr| pr.avatar),
            tex.as_ref(),
        );
        let tx = r.left() + 86.0;
        let (name, sub, stats) = match &me {
            Some(pr) => {
                let g: Vec<&str> = pr.tastes.iter().map(|&t| genre_name(self.language, t)).collect();
                let (l, pl) = (self.likes.len(), self.playlists.len());
                {
                    let liked_label = match self.language {
                        Language::En => {
                            if l == 1 {
                                "liked track"
                            } else {
                                "liked tracks"
                            }
                        }
                        Language::Fr => {
                            if l == 1 {
                                "titre aimé"
                            } else {
                                "titres aimés"
                            }
                        }
                        Language::It => {
                            if l == 1 {
                                "brano preferito"
                            } else {
                                "brani preferiti"
                            }
                        }
                        Language::Es => {
                            if l == 1 {
                                "canción favorita"
                            } else {
                                "canciones favoritas"
                            }
                        }
                        Language::Zh => "喜欢的曲目",
                        Language::Ru => {
                            if l == 1 {
                                "избранный трек"
                            } else {
                                "избранных треков"
                            }
                        }
                    };
                    let playlist_label = match self.language {
                        Language::En => {
                            if pl == 1 {
                                "playlist"
                            } else {
                                "playlists"
                            }
                        }
                        Language::Fr => {
                            if pl == 1 {
                                "playlist"
                            } else {
                                "playlists"
                            }
                        }
                        Language::It => {
                            if pl == 1 {
                                "playlist"
                            } else {
                                "playlist"
                            }
                        }
                        Language::Es => {
                            if pl == 1 {
                                "lista"
                            } else {
                                "listas"
                            }
                        }
                        Language::Zh => "播放列表",
                        Language::Ru => {
                            if pl == 1 {
                                "плейлист"
                            } else {
                                "плейлистов"
                            }
                        }
                    };
                    (
                        pr.name.clone(),
                        g.join(" · "),
                        format!("{} {} · {} {}", l, liked_label, pl, playlist_label),
                    )
                }
            }
            None => (
                tr(self.language, "guest").to_string(),
                tr(self.language, "guest_mode").to_string(),
                tr(self.language, "nothing_saved").to_string(),
            ),
        };
        fit(
            &p,
            pos2(tx, r.top() + 34.0),
            &name,
            FontId::proportional(18.0),
            TEXT,
            r.right() - tx - 16.0,
        );
        fit(
            &p,
            pos2(tx, r.top() + 56.0),
            &sub,
            FontId::proportional(12.0),
            MUTED,
            r.right() - tx - 16.0,
        );
        fit(
            &p,
            pos2(r.left() + 16.0, r.top() + 90.0),
            &stats,
            FontId::proportional(12.0),
            MUTED,
            mw - 32.0,
        );
        p.line_segment(
            [
                pos2(r.left() + 12.0, r.top() + 108.0),
                pos2(r.right() - 12.0, r.top() + 108.0),
            ],
            Stroke::new(1.0_f32, LINE),
        );

        let mut act: Option<PAct> = None;
        let mut y = r.top() + 112.0;
        if me.is_some() {
            let rr = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(mw - 12.0, 44.0));
            let rs = pm_row(ui, &p, rr, "pm_edit");
            p.text(
                pos2(rr.left() + 12.0, rr.center().y),
                Align2::LEFT_CENTER,
                tr(self.language, "edit_profile"),
                FontId::proportional(14.0),
                TEXT,
            );
            if rs.clicked() {
                act = Some(PAct::Edit);
            }
            y += 44.0;
        }

        spaced(
            &p,
            pos2(r.left() + 16.0, y + 16.0),
            Align2::LEFT_CENTER,
            tr(self.language, "theme"),
            10.0,
            MUTED,
        );
        y += 34.0;
        let cur_acc = ACCENT_RGB.load(AtOrd::Relaxed);
        let acy = y + 22.0;
        let mut chosen: Option<u32> = None;
        for (k, col) in ACCENT_SWATCHES.iter().enumerate() {
            let ctr = pos2(r.left() + 27.0 + k as f32 * 31.0, acy);
            let rs = ui.interact(
                Rect::from_center_size(ctr, vec2(28.0, 28.0)),
                Id::new(("pm_acc", k)),
                Sense::click(),
            );
            let t = hov(ui, &rs);
            p.circle_filled(ctr, 10.0 + 1.5 * t, rgb_of(*col));
            if *col == cur_acc {
                p.circle_stroke(ctr, 14.0, Stroke::new(1.5_f32, TEXT));
            }
            hand(ui, &rs);
            if rs.clicked() {
                chosen = Some(*col);
            }
        }
        let pc = pos2(r.right() - 30.0, acy);
        let prs = ui.interact(
            Rect::from_center_size(pc, vec2(30.0, 30.0)),
            Id::new("pm_acc_pick"),
            Sense::click(),
        );
        let pt = hov(ui, &prs);
        let custom = !ACCENT_SWATCHES.contains(&cur_acc);
        if custom {
            p.circle_filled(pc, 13.0, accent());
        }
        p.circle_stroke(
            pc,
            14.0,
            Stroke::new(
                if custom { 1.5_f32 } else { 1.0_f32 },
                if custom { TEXT } else { LINE.lerp_to_gamma(MUTED, pt) },
            ),
        );
        let pcol = if custom {
            on_accent()
        } else if self.acc_pick {
            accent()
        } else {
            MUTED.lerp_to_gamma(TEXT, pt)
        };
        pipette_icon(&p, pc, pcol);
        hand(ui, &prs);
        let prs = prs.on_hover_text(tr(self.language, "other_color"));
        if prs.clicked() {
            self.acc_pick = !self.acc_pick;
            if self.acc_pick {
                self.acc_hsv = rgb_to_hsv(cur_acc);
            }
        }
        if let Some(col) = chosen {
            self.set_accent(col);
            self.acc_hsv = rgb_to_hsv(col);
        }
        y += 44.0;

        spaced(
            &p,
            pos2(r.left() + 16.0, y + 16.0),
            Align2::LEFT_CENTER,
            "DISCORD",
            10.0,
            MUTED,
        );
        y += 34.0;
        {
            let rr = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(mw - 12.0, 44.0));
            let rs = pm_row(ui, &p, rr, "pm_discord").on_hover_text(tr(self.language, "discord_tip"));
            p.text(
                pos2(rr.left() + 12.0, rr.center().y),
                Align2::LEFT_CENTER,
                "Rich Presence",
                FontId::proportional(14.0),
                TEXT,
            );
            let t = hov_if(ui, Id::new("pm_discord_t"), self.dc_on);
            let sw = Rect::from_center_size(pos2(rr.right() - 30.0, rr.center().y), vec2(36.0, 20.0));
            p.rect_filled(sw, 10.0, SURF2.lerp_to_gamma(accent(), t));
            p.circle_filled(pos2(sw.left() + 10.0 + 16.0 * t, sw.center().y), 7.5, TEXT);
            if rs.clicked() {
                self.dc_on = !self.dc_on;
                save_discord_pref(self.dc_on);
            }
            y += 44.0;
        }

        {
            let rr = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(mw - 12.0, 44.0));
            let rs = pm_row(ui, &p, rr, "pm_language");
            p.text(
                pos2(rr.left() + 12.0, rr.center().y),
                Align2::LEFT_CENTER,
                tr(self.language, "language"),
                FontId::proportional(14.0),
                TEXT,
            );
            draw_language_flag(&p, pos2(rr.right() - 104.0, rr.center().y), self.language, 16.0);
            p.text(
                pos2(rr.right() - 34.0, rr.center().y),
                Align2::RIGHT_CENTER,
                self.language.name(),
                FontId::proportional(12.0),
                MUTED,
            );
            p.text(
                pos2(rr.right() - 10.0, rr.center().y),
                Align2::CENTER_CENTER,
                if self.lang_open { "‹" } else { "›" },
                FontId::proportional(16.0),
                MUTED,
            );
            hand(ui, &rs);
            if rs.clicked() {
                self.lang_open = !self.lang_open;
                self.acc_pick = false;
            }
            y += 44.0;
        }

        let lang_t = ui
            .ctx()
            .animate_bool_with_time(Id::new("lang_panel_open"), self.lang_open, 0.22);
        if lang_t > 0.01 {
            let lang_h = 44.0 + 6.0 * 46.0;
            let lang_t = lang_t as f32;

            let ease = 1.0 - (1.0 - lang_t).powi(3);
            let (lang_w, reveal_h) = (232.0 * ease, lang_h * ease);
            let lk = Rect::from_min_size(pos2(r.right() + 10.0, r.top() + 112.0), vec2(lang_w, reveal_h));
            let lp = ui.painter().with_clip_rect(lk);
            acrylic_panel(&lp, lk, 12.0);
            lp.rect_stroke(lk.shrink(0.5), 12.0, Stroke::new(1.0_f32, LINE));
            let ly = lk.top() + 10.0;
            lp.text(
                pos2(lk.left() + 14.0, ly + 17.0),
                Align2::LEFT_CENTER,
                tr(self.language, "language"),
                FontId::proportional(12.0),
                MUTED,
            );
            for (i, lang) in [
                Language::En,
                Language::Fr,
                Language::It,
                Language::Es,
                Language::Zh,
                Language::Ru,
            ]
            .iter()
            .enumerate()
            {
                let lr = Rect::from_min_size(
                    pos2(lk.left() + 8.0, ly + 34.0 + i as f32 * 46.0),
                    vec2(lk.width() - 16.0, 42.0),
                );
                if lk.width() > 180.0 {
                    let ls = pm_row(ui, &lp, lr, &format!("pm_lang_{i}"));
                    draw_language_flag(&lp, pos2(lr.left() + 23.0, lr.center().y), *lang, 18.0);
                    lp.text(
                        pos2(lr.left() + 52.0, lr.center().y),
                        Align2::LEFT_CENTER,
                        lang.name(),
                        FontId::proportional(14.0),
                        TEXT,
                    );
                    if *lang == self.language {
                        lp.circle_filled(pos2(lr.right() - 18.0, lr.center().y), 4.0, accent());
                    }
                    hand(ui, &ls);
                    if ls.clicked() {
                        self.language = *lang;
                        save_language(*lang);
                        if let Some(profile) = self.cur.and_then(|idx| self.profiles.get_mut(idx)) {
                            profile.language = lang.code().to_string();
                            save_profiles(&self.profiles);
                        }
                        self.lang_open = false;
                    }
                }
            }
        }
        let pk_h = 210.0_f32;
        let pk = Rect::from_min_size(
            pos2(
                r.right() + 10.0,
                (acy - 100.0).min(full.bottom() - 8.0 - pk_h).max(full.top() + 8.0),
            ),
            vec2(232.0, pk_h),
        );
        if n_sw > 0 {
            spaced(
                &p,
                pos2(r.left() + 16.0, y + 16.0),
                Align2::LEFT_CENTER,
                tr(self.language, "change_profile"),
                10.0,
                MUTED,
            );
            y += 34.0;
            for (k, (i, pr)) in others.iter().enumerate() {
                let rr = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(mw - 12.0, 46.0));
                let rs = pm_row(ui, &p, rr, &format!("pm_sw{k}"));
                let tex = self.photo_tex(&ctx, pr);
                draw_avatar(
                    &p,
                    pos2(rr.left() + 28.0, rr.center().y),
                    16.0,
                    Some(pr.avatar),
                    tex.as_ref(),
                );
                fit(
                    &p,
                    pos2(rr.left() + 54.0, rr.center().y),
                    &pr.name,
                    FontId::proportional(14.0),
                    TEXT,
                    rr.width() - 66.0,
                );
                if rs.clicked() {
                    act = Some(PAct::Switch(Some(*i)));
                }
                y += 46.0;
            }
            if me.is_some() {
                let rr = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(mw - 12.0, 46.0));
                let rs = pm_row(ui, &p, rr, "pm_guest");
                avatar_guest(&p, pos2(rr.left() + 28.0, rr.center().y), 16.0);
                p.text(
                    pos2(rr.left() + 54.0, rr.center().y),
                    Align2::LEFT_CENTER,
                    tr(self.language, "guest"),
                    FontId::proportional(14.0),
                    TEXT,
                );
                if rs.clicked() {
                    act = Some(PAct::Switch(None));
                }
                y += 46.0;
            }
        }
        let rr = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(mw - 12.0, 44.0));
        let rs = pm_row(ui, &p, rr, "pm_new");
        icon(&p, Ic::Plus, pos2(rr.left() + 22.0, rr.center().y), 14.0, accent());
        p.text(
            pos2(rr.left() + 44.0, rr.center().y),
            Align2::LEFT_CENTER,
            tr(self.language, "new_profile"),
            FontId::proportional(14.0),
            accent(),
        );
        if rs.clicked() {
            act = Some(PAct::New);
        }
        y += 44.0;

        if me.is_some() {
            let danger = Color32::from_rgb(239, 83, 80);
            let rr = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(mw - 12.0, 44.0));
            let rs = pm_row(ui, &p, rr, "pm_delete");
            if self.pm_del {
                p.rect_filled(rr, 8.0, danger.gamma_multiply(0.14));
            }
            let lab = if self.pm_del {
                tr(self.language, "confirmation_delete")
            } else {
                tr(self.language, "delete_profile")
            };
            p.text(
                pos2(rr.left() + 14.0, rr.center().y),
                Align2::LEFT_CENTER,
                lab,
                FontId::proportional(14.0),
                danger,
            );
            if rs.clicked() {
                if self.pm_del {
                    act = Some(PAct::Delete);
                } else {
                    self.pm_del = true;
                }
            }
        }

        if self.acc_pick {
            self.accent_picker(ui, pk);
        }

        match act {
            Some(PAct::Edit) => {
                if let Some(i) = self.cur {
                    self.open_form(Some(i));
                }
            }
            Some(PAct::Switch(w)) => {
                self.pui = Pui::Off;
                self.request_switch(ctx.input(|i| i.time), w);
            }
            Some(PAct::New) => self.open_form(None),
            Some(PAct::Delete) => {
                self.pm_del = false;
                if let Some(i) = self.cur {
                    self.delete_profile(i);
                }
            }
            None => {}
        }
    }
}
