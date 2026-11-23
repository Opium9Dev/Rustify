use super::super::*;

impl super::super::App {
    pub(in crate::app) fn library(&mut self, ui: &mut Ui, w: f32) {
        if self.tab == 1 {
            if let Some(i) = self.open_pl {
                if i < self.playlists.len() {
                    self.playlist_view(ui, w, i);
                    return;
                }
                self.open_pl = None;
            }
        }
        top(
            ui,
            w,
            match self.language {
                Language::En => "YOUR MUSIC",
                Language::Fr => "TA MUSIQUE",
                Language::It => "LA TUA MUSICA",
                Language::Es => "TU MÚSICA",
                Language::Zh => "你的音乐",
                Language::Ru => "ВАША МУЗЫКА",
            },
            tr(self.language, "library"),
        );
        chips(
            ui,
            w,
            &[tr(self.language, "liked"), tr(self.language, "playlists")],
            &mut self.tab,
            "tab",
        );
        ui.add_space(10.0);
        if self.tab == 0 {
            self.liked_list(ui, w);
        } else {
            self.playlists_grid(ui, w);
        }
    }

    pub(in crate::app) fn empty_note(ui: &mut Ui, w: f32, text: &str) {
        let (r, _) = ui.allocate_exact_size(vec2(w, 44.0), Sense::hover());
        ui.painter().text(
            r.left_center(),
            Align2::LEFT_CENTER,
            text,
            FontId::proportional(13.0),
            MUTED,
        );
    }

    pub(in crate::app) fn liked_list(&mut self, ui: &mut Ui, w: f32) {
        let n = self.likes.len();
        if section_btn(
            ui,
            w,
            tr(self.language, "liked"),
            tr(self.language, "read_all"),
            "likes_all",
            true,
        ) && n > 0
        {
            let l = self.likes.clone();
            self.play_list(l, 0);
        }
        if n == 0 {
            Self::empty_note(ui, w, tr(self.language, "no_liked"));
            return;
        }
        if let Some((a, idx, r)) = self.rows(ui, w, &self.likes, false) {
            let h = self.likes[idx].clone();
            if self.row_common(a, &h, r) == Some(Row::Open) {
                let l = self.likes.clone();
                self.play_or_toggle(l, idx);
            }
        }
    }

    pub(in crate::app) fn new_playlist_name(&self) -> String {
        let mut n = self.playlists.len() + 1;
        loop {
            let name = match self.language {
                Language::En => format!("Playlist {n}"),
                Language::Fr => format!("Playlist {n}"),
                Language::It => format!("Playlist {n}"),
                Language::Es => format!("Lista {n}"),
                Language::Zh => format!("播放列表 {n}"),
                Language::Ru => format!("Плейлист {n}"),
            };
            if !self.playlists.iter().any(|p| p.name == name) {
                return name;
            }
            n += 1;
        }
    }

    pub(in crate::app) fn commit_name(&mut self) {
        let name: String = self.pl_text.trim().chars().take(60).collect();
        if name.is_empty() {
            return;
        }
        match self.pl_edit {
            Some(PlEdit::New) => self.playlists.push(Playlist {
                name,
                tracks: Vec::new(),
            }),
            Some(PlEdit::Rename(i)) => {
                if let Some(p) = self.playlists.get_mut(i) {
                    p.name = name;
                }
            }
            None => return,
        }
        save_playlists(&self.playlists);
        self.pl_edit = None;
    }

    pub(in crate::app) fn name_bar(&mut self, ui: &mut Ui, w: f32) {
        let Some(edit) = self.pl_edit else { return };
        let (r, _) = ui.allocate_exact_size(vec2(w.min(460.0), 44.0), Sense::hover());
        let id = Id::new("pl_name_input");
        if self.pl_focus {
            ui.memory_mut(|m| m.request_focus(id));
            self.pl_focus = false;
        }
        let focused = ui.memory(|m| m.has_focus(id));
        let p = ui.painter().clone();
        p.rect_filled(r, 8.0, SURF);
        p.rect_stroke(r.shrink(0.5), 8.0, Stroke::new(1.0_f32, accent()));
        let name_rect = Rect::from_min_max(r.min + vec2(16.0, 0.0), r.max - vec2(100.0, 0.0));
        text_field(
            ui,
            name_rect,
            id,
            &mut self.pl_text,
            match self.language {
                Language::En => "Playlist name",
                Language::Fr => "Nom de la playlist",
                Language::It => "Nome playlist",
                Language::Es => "Nombre de la lista",
                Language::Zh => "播放列表名称",
                Language::Ru => "Название плейлиста",
            },
            &mut self.caret,
        );
        let ok = Rect::from_min_max(
            pos2(r.right() - 90.0, r.top() + 6.0),
            pos2(r.right() - 6.0, r.bottom() - 6.0),
        );
        let ors = ui.interact(ok, Id::new("pl_name_ok"), Sense::click());
        p.rect_filled(pop(ui, &ors, ok, 2.0), 6.0, accent());
        p.text(
            ok.center(),
            Align2::CENTER_CENTER,
            if edit == PlEdit::New {
                tr(self.language, "create")
            } else {
                "OK"
            },
            FontId::proportional(13.0),
            on_accent(),
        );
        hand(ui, &ors);
        if (focused && ui.input(|i| i.key_pressed(Key::Enter))) || ors.clicked() {
            self.commit_name();
        } else if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.pl_edit = None;
        }
    }

    pub(in crate::app) fn playlists_grid(&mut self, ui: &mut Ui, w: f32) {
        let (sr, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
        let nb = Rect::from_center_size(pos2(sr.right() - 88.0, sr.center().y), vec2(176.0, 38.0));
        let nrs = ui.interact(nb, Id::new("pl_new"), Sense::click());
        {
            let p = ui.painter().clone();
            p.text(
                sr.left_center(),
                Align2::LEFT_CENTER,
                tr(self.language, "your_playlists"),
                FontId::proportional(18.0),
                TEXT,
            );
            p.rect_filled(pop(ui, &nrs, nb, 3.0), 8.0, accent());
            icon(&p, Ic::Plus, pos2(nb.left() + 24.0, nb.center().y), 15.0, on_accent());
            p.text(
                pos2(nb.left() + 46.0, nb.center().y),
                Align2::LEFT_CENTER,
                tr(self.language, "new_playlist"),
                FontId::proportional(14.0),
                on_accent(),
            );
        }
        hand(ui, &nrs);
        if nrs.clicked() {
            self.pl_edit = Some(PlEdit::New);
            self.pl_text.clear();
            self.pl_focus = true;
        }
        if self.pl_edit == Some(PlEdit::New) {
            self.name_bar(ui, w);
            ui.add_space(14.0);
        }
        if self.playlists.is_empty() {
            Self::empty_note(
                ui,
                w,
                match self.language {
                    Language::En => "No playlists — create one or add a track with « + »",
                    Language::Fr => "Aucune playlist — crée-en une, ou ajoute un titre avec le bouton « + »",
                    Language::It => "Nessuna playlist — creane una o aggiungi un brano con « + »",
                    Language::Es => "No hay listas — crea una o añade una canción con « + »",
                    Language::Zh => "没有播放列表——创建一个或点击“+”添加曲目",
                    Language::Ru => "Нет плейлистов — создайте один или добавьте трек через «+»",
                },
            );
            return;
        }
        let n = (((w + 16.0) / 176.0) as usize).clamp(2, 8);
        let cw = (w - 16.0 * (n as f32 - 1.0)) / n as f32;
        let rh = cw + 46.0;
        let rows = (self.playlists.len() + n - 1) / n;
        let (gr, _) = ui.allocate_exact_size(
            vec2(w, rows as f32 * rh + rows.saturating_sub(1) as f32 * 16.0),
            Sense::hover(),
        );
        let p = ui.painter().clone();
        let (mut open, mut play): (Option<usize>, Option<usize>) = (None, None);
        let mut ctx_open: Option<(usize, Pos2)> = None;
        for (k, pl) in self.playlists.iter().enumerate() {
            let r = Rect::from_min_size(
                gr.min + vec2((k % n) as f32 * (cw + 16.0), (k / n) as f32 * (rh + 16.0)),
                vec2(cw, rh),
            );
            let rs = ui.interact(r, Id::new(("pl", k)), Sense::click());
            let cr = Rect::from_min_size(r.min, vec2(cw, cw));
            let dc = pos2(r.right() - 11.0, r.bottom() - 26.0);
            let ds = ui.interact(
                Rect::from_center_size(dc, vec2(24.0, 24.0)),
                Id::new(("pl_dots", k)),
                Sense::click(),
            );
            let fb = Rect::from_center_size(pos2(cr.right() - 26.0, cr.bottom() - 26.0), vec2(36.0, 36.0));
            let fs = ui.interact(fb, Id::new(("pl_play", k)), Sense::click());
            let cnt = pl.tracks.len();

            let t = hov_if(ui, rs.id, rs.hovered() || ds.hovered() || fs.hovered());
            playlist_cover(ui, &p, cr, &self.imgs, &pl.tracks, &pl.name, t);
            if t > 0.003 {
                p.rect_filled(cr, 8.0, Color32::from_black_alpha((70.0 * t) as u8));
            }
            fit(
                &p,
                pos2(r.left() + 1.0, r.bottom() - 26.0),
                &pl.name,
                FontId::proportional(13.0),
                TEXT,
                cw - 28.0,
            );
            let dt = hov(ui, &ds);
            if dt > 0.003 {
                p.rect_filled(
                    Rect::from_center_size(dc, vec2(24.0, 24.0)),
                    6.0,
                    SURF2.gamma_multiply(dt),
                );
            }
            icon(&p, Ic::More, dc, 14.0 + 2.0 * dt, MUTED.lerp_to_gamma(TEXT, dt));
            hand(ui, &ds);
            let sub = format!(
                "{} {}",
                cnt,
                if cnt == 1 {
                    tr(self.language, "track")
                } else {
                    tr(self.language, "tracks")
                }
            );
            fit(
                &p,
                pos2(r.left() + 1.0, r.bottom() - 10.0),
                &sub,
                FontId::proportional(11.0),
                MUTED,
                cw - 2.0,
            );
            let ft = if cnt > 0 { t } else { 0.0 };
            if ft > 0.003 {
                let fz = pop(ui, &fs, fb.translate(vec2(0.0, 8.0 * (1.0 - ft))), 3.0);
                p.rect_filled(fz, 8.0, accent().gamma_multiply(ft));
                icon(
                    &p,
                    Ic::Play,
                    fz.center(),
                    14.0 * fz.width() / fb.width(),
                    on_accent().gamma_multiply(ft),
                );
            }
            hand(ui, &rs);
            hand(ui, &fs);
            if ds.clicked() {
                ctx_open = Some((k, dc));
            } else if fs.clicked() && cnt > 0 {
                play = Some(k);
            } else if fs.clicked() || rs.clicked() {
                open = Some(k);
            }
        }
        if let Some(c) = ctx_open {
            self.pl_ctx = Some(c);
            self.pl_del = None;
        }
        if let Some(k) = play {
            let t = self.playlists[k].tracks.clone();
            self.play_list(t, 0);
        }
        if let Some(k) = open {
            self.open_pl = Some(k);
            self.pl_del = None;
            self.pl_edit = None;
        }
    }

    pub(in crate::app) fn text_btn(ui: &mut Ui, r: Rect, label: &str, id: &str, hot: bool) -> bool {
        let rs = ui.interact(r, Id::new(id), Sense::click());
        let t = hov(ui, &rs);
        let col = if hot { accent() } else { MUTED.lerp_to_gamma(TEXT, t) };
        let rz = pop(ui, &rs, r, 1.5);
        let p = ui.painter();
        p.rect_filled(rz, 8.0, SURF.gamma_multiply(t));
        p.rect_stroke(
            rz.shrink(0.5),
            8.0,
            Stroke::new(1.0_f32, if hot { col } else { LINE.lerp_to_gamma(MUTED, t) }),
        );
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(13.0),
            col,
        );
        hand(ui, &rs);
        rs.clicked()
    }

    pub(in crate::app) fn playlist_view(&mut self, ui: &mut Ui, w: f32, i: usize) {
        let (br, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
        let bk = ui.interact(
            Rect::from_min_size(br.min, vec2(150.0, 36.0)),
            Id::new("pl_back"),
            Sense::click(),
        );
        {
            let bt = hov(ui, &bk);
            let p = ui.painter();
            let col = MUTED.lerp_to_gamma(TEXT, bt);
            let c = pos2(br.left() + 6.0 - 3.0 * bt, br.center().y);
            let st = Stroke::new(1.6_f32, col);
            p.line_segment([c + vec2(3.0, -5.0), c + vec2(-2.0, 0.0)], st);
            p.line_segment([c + vec2(-2.0, 0.0), c + vec2(3.0, 5.0)], st);
            spaced(
                p,
                pos2(br.left() + 20.0 - 3.0 * bt, br.center().y),
                Align2::LEFT_CENTER,
                tr(self.language, "playlists"),
                11.0,
                col,
            );
        }
        hand(ui, &bk);
        if bk.clicked() {
            self.open_pl = None;
            self.pl_edit = None;
            self.pl_del = None;
            return;
        }

        let name = self.playlists[i].name.clone();
        let (hr, _) = ui.allocate_exact_size(vec2(w, 76.0), Sense::hover());
        spaced(
            ui.painter(),
            pos2(hr.left(), hr.top() + 14.0),
            Align2::LEFT_CENTER,
            tr(self.language, "playlist"),
            11.0,
            accent(),
        );
        if self.pl_edit == Some(PlEdit::Rename(i)) {
            let id = Id::new("pl_name_input");
            let seen = Id::new("pl_name_seen_focus");
            if self.pl_focus {
                ui.memory_mut(|m| m.request_focus(id));
                ui.memory_mut(|m| m.data.insert_temp(seen, false));
                self.pl_focus = false;
            }
            let fr = Rect::from_min_size(pos2(hr.left(), hr.top() + 26.0), vec2((w - 52.0).min(640.0), 40.0));
            text_field_sized(
                ui,
                fr,
                id,
                &mut self.pl_text,
                match self.language {
                    Language::En => "Playlist name",
                    Language::Fr => "Nom de la playlist",
                    Language::It => "Nome playlist",
                    Language::Es => "Nombre de la lista",
                    Language::Zh => "播放列表名称",
                    Language::Ru => "Название плейлиста",
                },
                &mut self.caret,
                30.0,
            );
            ui.painter()
                .line_segment([fr.left_bottom(), fr.right_bottom()], Stroke::new(1.5_f32, accent()));
            let focused = ui.memory(|m| m.has_focus(id));
            let was = ui.memory(|m| m.data.get_temp::<bool>(seen)).unwrap_or(false);
            if focused {
                ui.memory_mut(|m| m.data.insert_temp(seen, true));
            }
            if focused && ui.input(|i| i.key_pressed(Key::Enter)) {
                self.commit_name();
            } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                self.pl_edit = None;
            } else if was && !focused {
                if self.pl_text.trim().is_empty() {
                    self.pl_edit = None;
                } else {
                    self.commit_name();
                }
            }
        } else {
            fit(
                ui.painter(),
                pos2(hr.left(), hr.top() + 46.0),
                &name,
                FontId::proportional(30.0),
                TEXT,
                w - 52.0,
            );
        }

        let cnt = self.playlists[i].tracks.len();
        let total: u32 = self.playlists[i].tracks.iter().map(|t| t.secs).sum();
        let (ar, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
        let pb = Rect::from_min_size(pos2(ar.left(), ar.top() + 8.0), vec2(124.0, 40.0));
        let prs = ui.interact(pb, Id::new("pl_play_all"), Sense::click());
        {
            let p = ui.painter();
            let (bg, fg) = if cnt > 0 {
                (accent(), on_accent())
            } else {
                (SURF2, MUTED)
            };
            p.rect_filled(if cnt > 0 { pop(ui, &prs, pb, 3.0) } else { pb }, 8.0, bg);
            icon(p, Ic::Play, pos2(pb.left() + 26.0, pb.center().y), 14.0, fg);
            p.text(
                pos2(pb.left() + 46.0, pb.center().y),
                Align2::LEFT_CENTER,
                tr(self.language, "play"),
                FontId::proportional(14.0),
                fg,
            );
            let track_label = if cnt == 1 {
                tr(self.language, "track")
            } else {
                tr(self.language, "tracks")
            };
            let meta = format!(
                "{} {}  ·  {} {}",
                cnt,
                track_label,
                (total + 30) / 60,
                tr(self.language, "minutes")
            );
            spaced(
                p,
                pos2(ar.right(), ar.center().y),
                Align2::RIGHT_CENTER,
                &meta,
                11.0,
                MUTED,
            );
        }
        hand(ui, &prs);
        if prs.clicked() && cnt > 0 {
            let t = self.playlists[i].tracks.clone();
            self.play_list(t, 0);
        }
        let rn = Rect::from_min_size(pos2(pb.right() + 10.0, pb.top()), vec2(104.0, 40.0));
        if Self::text_btn(
            ui,
            rn,
            tr(self.language, "rename"),
            "pl_rename",
            self.pl_edit == Some(PlEdit::Rename(i)),
        ) {
            self.pl_edit = Some(PlEdit::Rename(i));
            self.pl_text = name.clone();
            self.pl_focus = true;
        }
        let armed = self.pl_del == Some(i);
        let dl = Rect::from_min_size(pos2(rn.right() + 10.0, pb.top()), vec2(116.0, 40.0));
        if Self::text_btn(
            ui,
            dl,
            if armed {
                tr(self.language, "confirm")
            } else {
                tr(self.language, "delete")
            },
            "pl_delete",
            armed,
        ) {
            if armed {
                self.playlists.remove(i);
                save_playlists(&self.playlists);
                self.open_pl = None;
                self.pl_del = None;
                self.pl_edit = None;
                return;
            }
            self.pl_del = Some(i);
        }
        ui.add_space(6.0);

        if cnt == 0 {
            Self::empty_note(ui, w, tr(self.language, "empty_playlist"));
            return;
        }
        if let Some((a, idx, r)) = self.pl_rows(ui, w, i) {
            let h = self.playlists[i].tracks[idx].clone();
            match self.row_common(a, &h, r) {
                Some(Row::Open) => {
                    let t = self.playlists[i].tracks.clone();
                    self.play_or_toggle(t, idx);
                }
                Some(Row::Remove) => {
                    self.playlists[i].tracks.remove(idx);
                    save_playlists(&self.playlists);
                }
                _ => {}
            }
        }
    }

    pub(in crate::app) fn playlist_ctx_menu(&mut self, ui: &mut Ui, area: Rect) {
        let Some((idx, at)) = self.pl_ctx else {
            ui.ctx().animate_value_with_time(Id::new("plc_h"), 0.0, 0.0);
            return;
        };
        if idx >= self.playlists.len() {
            self.pl_ctx = None;
            return;
        }
        let bd = ui.interact(area, Id::new("plc_backdrop"), Sense::click());
        if bd.clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
            self.pl_ctx = None;
            self.pl_del = None;
            return;
        }
        let (mw, rh) = (220.0_f32, 40.0_f32);
        let mh = 3.0 * rh + 12.0;
        let x = (at.x - mw + 14.0).min(area.right() - mw - 8.0).max(area.left() + 8.0);
        let below = at.y + 18.0 + mh <= area.bottom() - 8.0;
        let y = if below {
            at.y + 18.0
        } else {
            (at.y - 18.0 - mh).max(area.top() + 8.0)
        };
        let r = Rect::from_min_size(pos2(x, y), vec2(mw, mh));
        let hv = ui.ctx().animate_value_with_time(Id::new("plc_h"), mh, 0.18).min(mh);
        let vis = if below {
            Rect::from_min_size(r.min, vec2(mw, hv))
        } else {
            Rect::from_min_max(pos2(r.left(), r.bottom() - hv), r.max)
        };
        if hv < 1.0 {
            return;
        }
        ui.interact(vis, Id::new("plc_block"), Sense::click());
        let pf = ui.painter().clone();
        acrylic_panel(&pf, vis, 10.0);
        pf.rect_stroke(vis.shrink(0.5), 10.0, Stroke::new(1.0_f32, LINE));
        let p = pf.with_clip_rect(vis);

        let empty = self.playlists[idx].tracks.is_empty();
        let armed = self.pl_del == Some(idx);
        let labels = [
            tr(self.language, "play"),
            tr(self.language, "rename"),
            if armed {
                tr(self.language, "confirmation_delete")
            } else {
                tr(self.language, "delete")
            },
        ];
        let mut pick: Option<usize> = None;
        for (k, lab) in labels.iter().enumerate() {
            let rr = Rect::from_min_size(pos2(r.left() + 6.0, r.top() + 6.0 + k as f32 * rh), vec2(mw - 12.0, rh));
            if !vis.contains_rect(rr) {
                continue;
            }
            let rs = ui.interact(rr, Id::new(("plc_row", k)), Sense::click());
            let rt = hov(ui, &rs);
            if rt > 0.003 {
                p.rect_filled(rr, 8.0, LINE.gamma_multiply(rt));
            }
            let col = if k == 0 && empty {
                MUTED
            } else if k == 2 && armed {
                accent()
            } else {
                TEXT
            };
            p.text(
                pos2(rr.left() + 14.0, rr.center().y),
                Align2::LEFT_CENTER,
                *lab,
                FontId::proportional(14.0),
                col,
            );
            hand(ui, &rs);
            if rs.clicked() {
                pick = Some(k);
            }
        }
        match pick {
            Some(0) => {
                if !empty {
                    let t = self.playlists[idx].tracks.clone();
                    self.play_list(t, 0);
                    self.pl_ctx = None;
                }
            }
            Some(1) => {
                self.open_pl = Some(idx);
                self.pl_edit = Some(PlEdit::Rename(idx));
                self.pl_text = self.playlists[idx].name.clone();
                self.pl_focus = true;
                self.pl_del = None;
                self.pl_ctx = None;
            }
            Some(2) => {
                if armed {
                    self.playlists.remove(idx);
                    save_playlists(&self.playlists);
                    self.open_pl = None;
                    self.pl_edit = None;
                    self.pl_del = None;
                    self.pl_ctx = None;
                } else {
                    self.pl_del = Some(idx);
                }
            }
            _ => {}
        }
    }

    pub(in crate::app) fn playlist_menu(&mut self, ui: &mut Ui, area: Rect) {
        let Some(menu) = self.pl_menu.clone() else {
            ui.ctx().animate_value_with_time(Id::new("plm_h"), 0.0, 0.0);
            return;
        };
        let bd = ui.interact(area, Id::new("pl_backdrop"), Sense::click());
        if bd.clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
            self.pl_menu = None;
            return;
        }
        let n = self.playlists.len().min(8);
        let (mw, rh, hh) = (264.0_f32, 44.0_f32, 40.0_f32);
        let mh = hh + (n + 1) as f32 * rh + 8.0;
        let x = (menu.at.x - mw + 24.0)
            .min(area.right() - mw - 8.0)
            .max(area.left() + 8.0);
        let below = menu.at.y + 24.0 + mh <= area.bottom() - 8.0;
        let y = if below {
            menu.at.y + 24.0
        } else {
            (menu.at.y - 24.0 - mh).max(area.top() + 8.0)
        };
        let r = Rect::from_min_size(pos2(x, y), vec2(mw, mh));

        let hv = ui.ctx().animate_value_with_time(Id::new("plm_h"), mh, 0.18).min(mh);
        let vis = if below {
            Rect::from_min_size(r.min, vec2(mw, hv))
        } else {
            Rect::from_min_max(pos2(r.left(), r.bottom() - hv), r.max)
        };
        if hv < 1.0 {
            return;
        }
        ui.interact(vis, Id::new("pl_menu_block"), Sense::click());
        let pf = ui.painter().clone();
        acrylic_panel(&pf, vis, 10.0);
        pf.rect_stroke(vis.shrink(0.5), 10.0, Stroke::new(1.0_f32, LINE));
        let p = pf.with_clip_rect(vis);
        spaced(
            &p,
            pos2(r.left() + 16.0, r.top() + 20.0),
            Align2::LEFT_CENTER,
            tr(self.language, "add_to_playlist"),
            10.0,
            MUTED,
        );

        let mut toggle: Option<usize> = None;
        for k in 0..n {
            let rr = Rect::from_min_size(pos2(r.left() + 6.0, r.top() + hh + k as f32 * rh), vec2(mw - 12.0, rh));
            if !vis.contains_rect(rr) {
                continue;
            }
            let rs = ui.interact(rr, Id::new(("plm", k)), Sense::click());
            let rt = hov(ui, &rs);
            if rt > 0.003 {
                p.rect_filled(rr, 8.0, LINE.gamma_multiply(rt));
            }
            let pl = &self.playlists[k];
            let has = pl.tracks.iter().any(|t| t.id == menu.hit.id);
            fit(
                &p,
                pos2(rr.left() + 12.0, rr.center().y - 8.0),
                &pl.name,
                FontId::proportional(14.0),
                TEXT,
                rr.width() - 60.0,
            );
            let c = pl.tracks.len();
            let sub = format!(
                "{} {}",
                c,
                if c == 1 {
                    tr(self.language, "track")
                } else {
                    tr(self.language, "tracks")
                }
            );
            p.text(
                pos2(rr.left() + 12.0, rr.center().y + 9.0),
                Align2::LEFT_CENTER,
                sub,
                FontId::proportional(11.0),
                MUTED,
            );
            if has {
                icon(&p, Ic::Check, pos2(rr.right() - 22.0, rr.center().y), 14.0, accent());
            }
            hand(ui, &rs);
            if rs.clicked() {
                toggle = Some(k);
            }
        }
        let ly = r.top() + hh + n as f32 * rh;
        p.line_segment(
            [pos2(r.left() + 12.0, ly), pos2(r.right() - 12.0, ly)],
            Stroke::new(1.0_f32, LINE),
        );
        let rr = Rect::from_min_size(pos2(r.left() + 6.0, ly), vec2(mw - 12.0, rh));
        let ns = ui.interact(
            if vis.contains_rect(rr) { rr } else { Rect::NOTHING },
            Id::new("plm_new"),
            Sense::click(),
        );
        let nt = hov(ui, &ns);
        if nt > 0.003 {
            p.rect_filled(rr, 8.0, LINE.gamma_multiply(nt));
        }
        icon(&p, Ic::Plus, pos2(rr.left() + 22.0, rr.center().y), 14.0, accent());
        p.text(
            pos2(rr.left() + 44.0, rr.center().y),
            Align2::LEFT_CENTER,
            tr(self.language, "new_playlist"),
            FontId::proportional(14.0),
            accent(),
        );
        hand(ui, &ns);

        if let Some(k) = toggle {
            let pl = &mut self.playlists[k];
            if let Some(pos) = pl.tracks.iter().position(|t| t.id == menu.hit.id) {
                pl.tracks.remove(pos);
            } else {
                pl.tracks.push(menu.hit.clone());
            }
            save_playlists(&self.playlists);
        } else if ns.clicked() {
            let name = self.new_playlist_name();
            self.playlists.push(Playlist {
                name,
                tracks: vec![menu.hit.clone()],
            });
            save_playlists(&self.playlists);
        }
    }
}
