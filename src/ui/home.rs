use super::super::*;

impl super::super::App {
    pub(in crate::app) fn home(&mut self, ui: &mut Ui, w: f32) {
        let name = self.cur_profile().map(|p| p.name);
        let (hello, headline) = greeting(self.language, local_hour(), name.as_deref(), self.greet_seed);
        top(ui, w, &hello, &headline);
        let chips_labels = [
            tr(self.language, "for_me"),
            match self.language {
                Language::En => "Focus",
                Language::Fr => "Focus",
                Language::It => "Concentrazione",
                Language::Es => "Enfoque",
                Language::Zh => "专注",
                Language::Ru => "Концентрация",
            },
            match self.language {
                Language::En => "Chill",
                Language::Fr => "Chill",
                Language::It => "Relax",
                Language::Es => "Relajado",
                Language::Zh => "轻松",
                Language::Ru => "Релакс",
            },
            match self.language {
                Language::En => "Energy",
                Language::Fr => "Énergie",
                Language::It => "Energia",
                Language::Es => "Energía",
                Language::Zh => "活力",
                Language::Ru => "Энергия",
            },
            match self.language {
                Language::En => "Night",
                Language::Fr => "Nuit",
                Language::It => "Notte",
                Language::Es => "Noche",
                Language::Zh => "夜晚",
                Language::Ru => "Ночь",
            },
            match self.language {
                Language::En => "Sunshine",
                Language::Fr => "Soleil",
                Language::It => "Sole",
                Language::Es => "Sol",
                Language::Zh => "阳光",
                Language::Ru => "Солнце",
            },
        ];
        chips(ui, w, &chips_labels, &mut self.mood, "mood");
        ui.add_space(12.0);
        let ctx = ui.ctx().clone();
        let fq = self.for_you_query();
        self.home_load(&ctx, &fq);
        self.home_load(&ctx, TOP_QUERY);
        let fy = self.home_cache.get(&fq).cloned();
        let tl = self.home_cache.get(TOP_QUERY).cloned();

        self.hero(ui, w);
        ui.add_space(22.0);
        let lab = if self.all_for_you {
            tr(self.language, "collapse")
        } else {
            tr(self.language, "show_all")
        };
        if section_btn(ui, w, tr(self.language, "for_you"), lab, "fy_all", false) {
            self.all_for_you = !self.all_for_you;
        }
        self.cards(ui, w, &fq, fy);

        for t in self.taste_rows() {
            ui.add_space(22.0);
            let q = taste_query(t);
            self.home_load(&ctx, &q);
            let l = self.home_cache.get(&q).cloned();
            let mix_label = match self.language {
                Language::En => "Your mix",
                Language::Fr => "Ton mix",
                Language::It => "Il tuo mix",
                Language::Es => "Tu mezcla",
                Language::Zh => "你的混合歌单",
                Language::Ru => "Ваш микс",
            };
            section(ui, w, &format!("{} {}", mix_label, genre_name(self.language, t)), "");
            self.cards(ui, w, &q, l);
        }
        ui.add_space(22.0);
        if section_btn(
            ui,
            w,
            tr(self.language, "top_week"),
            tr(self.language, "read_all"),
            "top_all",
            true,
        ) {
            if let Some(l) = &tl {
                self.play_list(l.iter().take(10).cloned().collect(), 0);
            }
        }
        self.top_list(ui, w, tl);
    }

    pub(in crate::app) fn load_failed(&mut self, ui: &mut Ui, w: f32, key: &str) {
        let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        let rs = ui.interact(r, Id::new(("retry", key)), Sense::click());
        let e = self.home_err.get(key).cloned().unwrap_or_default();
        ui.painter().text(
            r.left_center(),
            Align2::LEFT_CENTER,
            format!(
                "{} ({e}) — {}",
                tr(self.language, "load_error"),
                tr(self.language, "retry")
            ),
            FontId::proportional(13.0),
            accent().lerp_to_gamma(TEXT, hov(ui, &rs)),
        );
        hand(ui, &rs);
        if rs.clicked() {
            self.home_err.remove(key);
        }
    }

    pub(in crate::app) fn hero(&mut self, ui: &mut Ui, w: f32) {
        let open_t = ui
            .ctx()
            .animate_bool_with_time(Id::new("ly_open"), self.ly_open && self.started, 0.3);
        let open_t = open_t * open_t * (3.0 - 2.0 * open_t);
        let (r, _) = ui.allocate_exact_size(vec2(w, HERO_H + LY_H * open_t), Sense::hover());
        let top_r = Rect::from_min_size(r.min, vec2(r.width(), HERO_H));
        let h = self.now();
        let seed = self.now_seed();

        let ld = if h.thumb.is_empty() {
            None
        } else {
            self.imgs.get_full(&h.thumb)
        };

        let target = if !self.started {
            c(34, 34, 40)
        } else {
            ld.as_ref().map(|l| l.2).unwrap_or(PAL[seed % PAL.len()][0])
        };
        let dt = ui.input(|i| i.stable_dt);
        let cur = *self.hero_bg.get_or_insert(target);
        let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
        let near = d(cur.r(), target.r()) + d(cur.g(), target.g()) + d(cur.b(), target.b()) <= 12;
        let bg = if near {
            target
        } else {
            ui.ctx().request_repaint();
            cur.lerp_to_gamma(target, 1.0 - (-dt * 8.0).exp())
        };
        self.hero_bg = Some(bg);

        let bg = bg.lerp_to_gamma(BG, 0.40);
        let fg = readable(bg);

        let time = ui.input(|i| i.time) as f32;
        let p = ui.painter().clone();
        p.rect_filled(
            r,
            12.0,
            bg.gamma_multiply(if GLASS_ON.load(AtOrd::Relaxed) { 0.72 } else { 1.0 }),
        );

        if let Some((_, blur, _)) = &ld {
            let s = (r.height() * 1.5).max(r.width() * 0.28);
            let x0 = r.right() - s * 0.96;
            let y0 = r.center().y - s / 2.0;

            hero_image(&p, r, 12.0, blur.id(), x0, y0, s);
        } else if self.started && h.thumb.is_empty() {
            let cs = r.height() - 32.0;
            cover(
                &p,
                Rect::from_min_size(pos2(r.right() - cs - 16.0, r.top() + 16.0), vec2(cs, cs)),
                seed,
                8.0,
                true,
            );
        }
        p.rect_stroke(r.shrink(0.5), 11.5, Stroke::new(1.0_f32, HAIR));
        let tw = (r.width() - 56.0) * 0.6;

        let x = r.left() + 28.0;
        spaced(
            &p,
            pos2(x, r.top() + 32.0),
            Align2::LEFT_CENTER,
            if self.started {
                tr(self.language, "listening")
            } else {
                tr(self.language, "nothing")
            },
            11.0,
            fg,
        );
        for k in 0..(if self.started { 4 } else { 0 }) {
            let hh = if self.playing {
                4.0 + 10.0 * (time * 4.0 + k as f32 * 1.4).sin().abs()
            } else {
                4.0
            };
            p.rect_filled(
                Rect::from_center_size(pos2(x + 112.0 + k as f32 * 5.0, r.top() + 32.0), vec2(2.5, hh)),
                0.0,
                fg,
            );
        }
        let title = if self.started {
            h.title.clone()
        } else {
            tr(self.language, "choose_track").to_string()
        };
        fit(&p, pos2(x, r.top() + 78.0), &title, FontId::proportional(34.0), fg, tw);
        let sub = if !self.started {
            tr(self.language, "start_from").to_string()
        } else if h.album.is_empty() || h.album == h.title {
            h.artist.clone()
        } else {
            format!("{}  ·  {}", h.artist, h.album)
        };
        fit(
            &p,
            pos2(x, r.top() + 114.0),
            &sub,
            FontId::proportional(14.0),
            fg.gamma_multiply(0.8),
            tw,
        );

        let pb = Rect::from_min_size(pos2(x, top_r.bottom() - 62.0), vec2(124.0, 40.0));
        let rs = ui.interact(pb, Id::new("hero_play"), Sense::click());
        p.rect_filled(pop(ui, &rs, pb, 3.0), 8.0, fg);
        icon(
            &p,
            if self.playing { Ic::Pause } else { Ic::Play },
            pos2(pb.left() + 26.0, pb.center().y),
            14.0,
            bg,
        );
        p.text(
            pos2(pb.left() + 46.0, pb.center().y),
            Align2::LEFT_CENTER,
            if self.playing {
                tr(self.language, "pause")
            } else {
                tr(self.language, "play")
            },
            FontId::proportional(14.0),
            bg,
        );
        hand(ui, &rs);
        if rs.clicked() {
            self.toggle_play();
        }

        if !self.started {
            return;
        }
        let hb = Rect::from_center_size(pos2(pb.right() + 30.0, pb.center().y), vec2(40.0, 40.0));
        let hs = ui.interact(hb, Id::new("hero_like"), Sense::click());
        let hz = pop(ui, &hs, hb, 2.5);
        p.rect_filled(hz, 8.0, fg.gamma_multiply(0.16 * hov(ui, &hs)));
        p.rect_stroke(hz, 8.0, Stroke::new(1.5_f32, fg));
        icon(
            &p,
            Ic::Heart(self.is_liked()),
            hb.center(),
            15.0 * hz.width() / hb.width(),
            fg,
        );
        hand(ui, &hs);
        if hs.clicked() {
            self.toggle_like();
        }

        let ab = Rect::from_center_size(pos2(hb.right() + 28.0, pb.center().y), vec2(40.0, 40.0));
        let ars = ui.interact(ab, Id::new("hero_add"), Sense::click());
        let az = pop(ui, &ars, ab, 2.5);
        p.rect_filled(az, 8.0, fg.gamma_multiply(0.16 * hov(ui, &ars)));
        p.rect_stroke(az, 8.0, Stroke::new(1.5_f32, fg));
        icon(&p, Ic::Plus, ab.center(), 15.0 * az.width() / ab.width(), fg);
        hand(ui, &ars);
        if ars.clicked() {
            self.pl_menu = Some(PlMenu {
                hit: h.clone(),
                at: ab.center(),
            });
        }
        ars.on_hover_text(tr(self.language, "add_playlist"));

        let lb = Rect::from_center_size(pos2(ab.right() + 28.0, pb.center().y), vec2(40.0, 40.0));
        let lrs = ui.interact(lb, Id::new("hero_lyrics"), Sense::click());
        let lz = pop(ui, &lrs, lb, 2.5);
        let on = self.ly_open;
        p.rect_filled(
            lz,
            8.0,
            if on {
                fg
            } else {
                fg.gamma_multiply(0.16 * hov(ui, &lrs))
            },
        );
        p.rect_stroke(lz, 8.0, Stroke::new(1.5_f32, fg));
        icon(
            &p,
            Ic::Lyrics,
            lb.center(),
            15.0 * lz.width() / lb.width(),
            if on { bg } else { fg },
        );
        hand(ui, &lrs);
        if lrs.clicked() {
            self.ly_open = !self.ly_open;
        }
        lrs.on_hover_text(if on {
            tr(self.language, "hide_lyrics")
        } else {
            tr(self.language, "lyrics")
        });

        if open_t > 0.01 {
            let area = Rect::from_min_max(
                pos2(r.left() + 28.0, top_r.bottom() + 6.0),
                pos2(r.right() - 28.0, top_r.bottom() + LY_H - 22.0),
            );
            let pr = p.with_clip_rect(r.intersect(p.clip_rect()));
            pr.line_segment(
                [
                    pos2(area.left(), top_r.bottom() - 4.0),
                    pos2(area.right(), top_r.bottom() - 4.0),
                ],
                Stroke::new(1.0_f32, fg.gamma_multiply(0.18 * open_t)),
            );
            self.lyrics_panel(ui, &pr, area, fg, open_t);
        }
    }

    pub(in crate::app) fn cards(&mut self, ui: &mut Ui, w: f32, key: &str, list: Option<Arc<Vec<Hit>>>) {
        let n = (((w + 16.0) / 176.0) as usize).clamp(2, 8);
        let cw = (w - 16.0 * (n as f32 - 1.0)) / n as f32;
        let rh = cw + 46.0;
        let Some(list) = list else {
            if self.home_err.contains_key(key) {
                self.load_failed(ui, w, key);
                return;
            }
            let (gr, _) = ui.allocate_exact_size(vec2(w, rh), Sense::hover());
            for k in 0..n {
                let r = Rect::from_min_size(gr.min + vec2(k as f32 * (cw + 16.0), 0.0), vec2(cw, rh));
                skel(ui, Rect::from_min_size(r.min, vec2(cw, cw)), 8.0);
                skel(
                    ui,
                    Rect::from_min_size(r.min + vec2(0.0, cw + 12.0), vec2(cw * 0.7, 10.0)),
                    3.0,
                );
                skel(
                    ui,
                    Rect::from_min_size(r.min + vec2(0.0, cw + 30.0), vec2(cw * 0.45, 8.0)),
                    3.0,
                );
            }
            return;
        };
        let items: Vec<Hit> = list.iter().take(16).cloned().collect();
        if items.is_empty() {
            let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
            ui.painter().text(
                r.left_center(),
                Align2::LEFT_CENTER,
                tr(self.language, "no_track_moment"),
                FontId::proportional(13.0),
                MUTED,
            );
            return;
        }
        let shown = if self.all_for_you && key == self.for_you_query().as_str() {
            items.len()
        } else {
            n.min(items.len())
        };
        let rows = (shown + n - 1) / n;
        let (gr, _) = ui.allocate_exact_size(
            vec2(w, rows as f32 * rh + rows.saturating_sub(1) as f32 * 16.0),
            Sense::hover(),
        );
        let p = ui.painter().clone();
        let now_id = self.now_id();
        for k in 0..shown {
            let h = &items[k];
            let r = Rect::from_min_size(
                gr.min + vec2((k % n) as f32 * (cw + 16.0), (k / n) as f32 * (rh + 16.0)),
                vec2(cw, rh),
            );
            let rs = ui.interact(r, Id::new(("card", key, k)), Sense::click());
            let cr = Rect::from_min_size(r.min, vec2(cw, cw));
            let cur = now_id.as_deref() == Some(h.id.as_str());
            let big = h.thumb.replace("w96-h96", "w300-h300");
            let t = hov(ui, &rs);

            art_zoom(ui, &p, cr, &self.imgs, &[&big, &h.thumb], seed_of(&h.id), 8.0, t);
            if t > 0.003 {
                p.rect_filled(cr, 8.0, Color32::from_black_alpha((70.0 * t) as u8));
            }
            if cur {
                p.rect_stroke(cr, 8.0, Stroke::new(2.0_f32, accent()));
            }
            fit(
                &p,
                pos2(r.left() + 1.0, r.bottom() - 26.0),
                &h.title,
                FontId::proportional(13.0),
                if cur { accent() } else { TEXT },
                cw - 2.0,
            );
            fit(
                &p,
                pos2(r.left() + 1.0, r.bottom() - 10.0),
                &h.artist,
                FontId::proportional(11.0),
                MUTED,
                cw - 2.0,
            );
            let ft = if cur { 1.0 } else { t };
            if ft > 0.003 {
                let fb = Rect::from_center_size(
                    pos2(cr.right() - 26.0, cr.bottom() - 26.0 + 8.0 * (1.0 - ft)),
                    vec2(36.0, 36.0),
                );
                p.rect_filled(fb, 8.0, accent().gamma_multiply(ft));
                icon(
                    &p,
                    if cur && self.playing { Ic::Pause } else { Ic::Play },
                    fb.center(),
                    14.0,
                    on_accent().gamma_multiply(ft),
                );
            }
            hand(ui, &rs);
            if rs.clicked() {
                if cur {
                    self.playing = !self.playing;
                } else {
                    self.play_list(items.clone(), k);
                }
            }
        }
    }

    pub(in crate::app) fn top_list(&mut self, ui: &mut Ui, w: f32, list: Option<Arc<Vec<Hit>>>) {
        let two = w > 760.0;
        let per = if two { 5 } else { 10 };
        let cw = if two { (w - 24.0) / 2.0 } else { w };
        let Some(list) = list else {
            if self.home_err.contains_key(TOP_QUERY) {
                self.load_failed(ui, w, TOP_QUERY);
                return;
            }
            let (gr, _) = ui.allocate_exact_size(vec2(w, per as f32 * 56.0), Sense::hover());
            for k in 0..10 {
                let r = Rect::from_min_size(
                    gr.min + vec2((k / per) as f32 * (cw + 24.0), (k % per) as f32 * 56.0),
                    vec2(cw, 52.0),
                );
                skel(ui, Rect::from_min_size(r.min + vec2(46.0, 6.0), vec2(40.0, 40.0)), 5.0);
                skel(
                    ui,
                    Rect::from_min_size(r.min + vec2(98.0, 12.0), vec2(cw * 0.4, 10.0)),
                    3.0,
                );
                skel(
                    ui,
                    Rect::from_min_size(r.min + vec2(98.0, 30.0), vec2(cw * 0.25, 8.0)),
                    3.0,
                );
            }
            return;
        };
        let items: Vec<Hit> = list.iter().take(10).cloned().collect();
        if items.is_empty() {
            let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
            ui.painter().text(
                r.left_center(),
                Align2::LEFT_CENTER,
                tr(self.language, "no_track_moment"),
                FontId::proportional(13.0),
                MUTED,
            );
            return;
        }
        let rows = per.min(items.len());
        let (gr, _) = ui.allocate_exact_size(vec2(w, rows as f32 * 56.0), Sense::hover());
        let now_id = self.now_id();
        let mut act: Option<(Row, usize, Rect)> = None;
        for (k, h) in items.iter().enumerate() {
            let r = Rect::from_min_size(
                gr.min + vec2((k / per) as f32 * (cw + 24.0), (k % per) as f32 * 56.0),
                vec2(cw, 52.0),
            );
            let on = now_id.as_deref() == Some(h.id.as_str());
            if let Some(a) = Self::hit_row(
                ui,
                r,
                h,
                k,
                on,
                &self.imgs,
                on && self.playing,
                self.liked_ids.contains(&h.id),
                false,
            ) {
                act = Some((a, k, r));
            }
        }
        if let Some((a, k, r)) = act {
            if self.row_common(a, &items[k], r) == Some(Row::Open) {
                self.play_or_toggle(items.clone(), k);
            }
        }
    }
}
