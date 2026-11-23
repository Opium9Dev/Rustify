use super::super::*;

impl super::super::App {
    pub(in crate::app) fn search(&mut self, ui: &mut Ui, w: f32) {
        top(
            ui,
            w,
            match self.language {
                Language::En => "EXPLORE",
                Language::Fr => "EXPLORER",
                Language::It => "ESPLORA",
                Language::Es => "EXPLORAR",
                Language::Zh => "探索",
                Language::Ru => "ОБЗОР",
            },
            tr(self.language, "search"),
        );
        let (pr, _) = ui.allocate_exact_size(vec2(w.min(520.0), 44.0), Sense::hover());
        let sid = Id::new("search_input");
        let bar_rs = ui.interact(pr, Id::new("search_bar"), Sense::click());
        if bar_rs.clicked() {
            ui.memory_mut(|m| m.request_focus(sid));
        }
        let focused = ui.memory(|m| m.has_focus(sid));
        ui.painter().rect_filled(pr, 8.0, SURF);

        let bh = hov_if(ui, Id::new("search_bar_hov"), ui.rect_contains_pointer(pr));
        let bf = hov_if(ui, Id::new("search_bar_focus"), focused);
        ui.painter()
            .rect_stroke(pr, 8.0, Stroke::new(1.0_f32, LINE.gamma_multiply(bh * (1.0 - bf))));
        ui.painter()
            .rect_stroke(pr, 8.0, Stroke::new(1.0_f32, accent().gamma_multiply(bf)));
        icon(
            ui.painter(),
            Ic::Search,
            pr.left_center() + vec2(24.0, 0.0),
            16.0,
            MUTED.lerp_to_gamma(TEXT, bf),
        );
        let tf_rect = Rect::from_min_max(pr.min + vec2(48.0, 0.0), pr.max - vec2(40.0, 0.0));
        text_field(
            ui,
            tf_rect,
            sid,
            &mut self.query,
            match self.language {
                Language::En => "Tracks, artists, albums…",
                Language::Fr => "Titres, artistes, albums…",
                Language::It => "Brani, artisti, album…",
                Language::Es => "Canciones, artistas, álbumes…",
                Language::Zh => "曲目、歌手、专辑…",
                Language::Ru => "Треки, исполнители, альбомы…",
            },
            &mut self.caret,
        );
        if !self.query.is_empty() {
            let cc = pr.right_center() - vec2(20.0, 0.0);
            let crs = ui.interact(
                Rect::from_center_size(cc, vec2(28.0, 28.0)),
                Id::new("search_clear"),
                Sense::click(),
            );
            let ct = hov(ui, &crs);
            ui.painter().circle_filled(cc, 13.0 * ct, SURF2);
            let st = Stroke::new(1.6_f32, MUTED.lerp_to_gamma(TEXT, ct));
            let cz = 1.0 + 0.2 * ct;
            ui.painter()
                .line_segment([cc + vec2(-4.5, -4.5) * cz, cc + vec2(4.5, 4.5) * cz], st);
            ui.painter()
                .line_segment([cc + vec2(-4.5, 4.5) * cz, cc + vec2(4.5, -4.5) * cz], st);
            hand(ui, &crs);
            if crs.clicked() {
                self.query.clear();
            }
        }

        if focused && !self.query.trim().is_empty() && ui.input(|i| i.key_pressed(Key::Enter)) {
            let q = self.query.clone();
            self.push_recent(&q);
            self.edit_t = ui.input(|i| i.time) - 1.0;
        }
        self.tick_search(ui.ctx());
        ui.add_space(22.0);
        if self.query.trim().is_empty() {
            section(ui, w, "Parcourir par genre", "12 GENRES");
            let cols = (((w + 12.0) / 200.0) as usize).clamp(2, 6);
            let tw = (w - 12.0 * (cols as f32 - 1.0)) / cols as f32;
            let rows = (GENRES.len() + cols - 1) / cols;
            let (gr, _) = ui.allocate_exact_size(vec2(w, rows as f32 * 104.0), Sense::hover());
            for i in 0..GENRES.len() {
                let g = genre_name(self.language, i);
                let r = Rect::from_min_size(
                    gr.min + vec2((i % cols) as f32 * (tw + 12.0), (i / cols) as f32 * 104.0),
                    vec2(tw, 92.0),
                );
                let rs = ui.interact(r, Id::new(("genre", i)), Sense::click());
                let pal = PAL[(i * 3 + 1) % PAL.len()];
                let gt = hov(ui, &rs);
                ui.painter().rect_filled(pop(ui, &rs, r, 4.0), 10.0, pal[0]);

                let gi = Rect::from_center_size(
                    pos2(r.right() - 40.0, r.top() + 40.0 - 2.0 * gt),
                    vec2(52.0, 52.0) * (1.0 + 0.16 * gt),
                );
                genre_icon(ui.painter(), gi, i, pal[1], pal[0]);
                ui.painter().text(
                    pos2(r.left() + 16.0, r.bottom() - 22.0),
                    Align2::LEFT_CENTER,
                    g,
                    FontId::proportional(18.0),
                    pal[1],
                );
                hand(ui, &rs);
                if rs.clicked() {
                    self.query = GENRE_TERMS[i].to_string();
                    self.seen_query = self.query.clone();
                    self.edit_t = ui.input(|i| i.time) - 1.0;
                    ui.ctx().request_repaint();
                }
            }
            self.history_list(ui, w);
        } else {
            let right = if self.searching {
                tr(self.language, "searching").to_string()
            } else {
                format!(
                    "{} {}",
                    self.hits.len(),
                    match self.language {
                        Language::En => "TRACKS",
                        Language::Fr => "TITRES",
                        Language::It => "BRANI",
                        Language::Es => "CANCIONES",
                        Language::Zh => "曲目",
                        Language::Ru => "ТРЕКОВ",
                    }
                )
            };
            section(ui, w, tr(self.language, "results"), &right);
            if let Some(e) = &self.search_err {
                let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
                ui.painter().text(
                    r.left_center(),
                    Align2::LEFT_CENTER,
                    format!("Error: {e}"),
                    FontId::proportional(13.0),
                    accent(),
                );
            } else if !self.searching && self.hits.is_empty() && self.sent_query == self.query.trim() {
                let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
                ui.painter().text(
                    r.left_center(),
                    Align2::LEFT_CENTER,
                    tr(self.language, "no_results"),
                    FontId::proportional(13.0),
                    MUTED,
                );
            }
            let mut act: Option<(Row, Hit, Rect)> = None;
            for (i, h) in self.hits.iter().enumerate() {
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
                    let q = self.query.clone();
                    self.push_recent(&q);
                    self.push_history(&h);
                }
            }
        }

        self.recent_dropdown(ui, pr, focused, sid);
    }

    pub(in crate::app) fn hit_row(
        ui: &mut Ui,
        r: Rect,
        h: &Hit,
        i: usize,
        selected: bool,
        imgs: &ImgCache,
        eq: bool,
        liked: bool,
        removable: bool,
    ) -> Option<Row> {
        let rs = ui.interact(
            r,
            Id::new(("hit", i)),
            if removable {
                Sense::click_and_drag()
            } else {
                Sense::click()
            },
        );
        let btn = |dx: f32| Rect::from_center_size(pos2(r.right() - dx, r.center().y), vec2(28.0, 28.0));
        let hs = ui.interact(btn(HEART_DX), Id::new(("hit_like", i)), Sense::click());
        let ps = ui.interact(btn(ADD_DX), Id::new(("hit_add", i)), Sense::click());
        let xs = removable.then(|| ui.interact(btn(RM_DX), Id::new(("hit_rm", i)), Sense::click()));
        let hot = rs.hovered() || hs.hovered() || ps.hovered() || xs.as_ref().is_some_and(|x| x.hovered());
        hover_prefetch(ui, &h.id, rs.hovered());
        let ht = hov_if(ui, rs.id, hot);
        let (hts, pts) = (hov(ui, &hs), hov(ui, &ps));
        let xts = xs.as_ref().map_or(0.0, |x| hov(ui, x));
        let p = ui.painter().clone();
        if selected {
            p.rect_filled(r, 8.0, SURF);
        } else if ht > 0.003 {
            p.rect_filled(r, 8.0, SURF.gamma_multiply(ht));
        }
        if eq {
            let time = ui.input(|i| i.time) as f32;
            for b in 0..3 {
                let hh = 4.0 + 10.0 * (time * 5.0 + b as f32 * 1.6).sin().abs();
                p.rect_filled(
                    Rect::from_center_size(
                        pos2(r.left() + 24.0 + (b as f32 - 1.0) * 6.0, r.center().y),
                        vec2(3.0, hh),
                    ),
                    0.0,
                    accent(),
                );
            }
        } else {
            p.text(
                pos2(r.left() + 24.0, r.center().y),
                Align2::CENTER_CENTER,
                format!("{:02}", i + 1),
                FontId::proportional(13.0),
                if selected { accent() } else { MUTED },
            );
        }
        if removable && ht > 0.003 {
            for gx in [0.0_f32, 4.0] {
                for gy in [-4.0_f32, 0.0, 4.0] {
                    p.circle_filled(
                        pos2(r.left() + 7.0 + gx, r.center().y + gy),
                        1.1,
                        MUTED.gamma_multiply(ht),
                    );
                }
            }
        }
        let cr = Rect::from_min_size(r.min + vec2(46.0, 6.0), vec2(40.0, 40.0));
        art_zoom(ui, &p, cr, imgs, &[&h.thumb], seed_of(&h.id), 5.0, ht);
        let wide = r.width() > 560.0;
        let album_x = r.left() + r.width() * 0.52;
        let right_lim = r.right() - if removable { RM_DX + 24.0 } else { ADD_DX + 24.0 };
        let tx = r.left() + 98.0;
        let text_w = if wide { album_x - 20.0 - tx } else { right_lim - tx };
        fit(
            &p,
            pos2(tx, r.center().y - 8.0),
            &h.title,
            FontId::proportional(14.0),
            if selected { accent() } else { TEXT },
            text_w,
        );
        fit(
            &p,
            pos2(tx, r.center().y + 9.0),
            &h.artist,
            FontId::proportional(12.0),
            MUTED,
            text_w,
        );
        if wide {
            fit(
                &p,
                pos2(album_x, r.center().y),
                &h.album,
                FontId::proportional(13.0),
                MUTED,
                right_lim - album_x,
            );
        }
        if h.secs > 0 {
            p.text(
                pos2(r.right() - 16.0, r.center().y),
                Align2::RIGHT_CENTER,
                mmss(h.secs as f32),
                FontId::proportional(12.0),
                MUTED,
            );
        }

        if liked || ht > 0.003 {
            let hc = pos2(r.right() - HEART_DX, r.center().y);
            p.circle_filled(hc, 14.0 * hts, SURF2);
            let col = if liked {
                accent()
            } else {
                MUTED.lerp_to_gamma(TEXT, hts).gamma_multiply(ht)
            };
            icon(&p, Ic::Heart(liked), hc, 14.0 + 3.0 * hts, col);
        }
        if ht > 0.003 {
            let pc = pos2(r.right() - ADD_DX, r.center().y);
            p.circle_filled(pc, 14.0 * pts, SURF2);
            icon(
                &p,
                Ic::Plus,
                pc,
                15.0 + 3.0 * pts,
                MUTED.lerp_to_gamma(TEXT, pts).gamma_multiply(ht),
            );
            if xs.is_some() {
                let c = pos2(r.right() - RM_DX, r.center().y);
                p.circle_filled(c, 14.0 * xts, SURF2);
                let st = Stroke::new(1.6_f32, MUTED.lerp_to_gamma(TEXT, xts).gamma_multiply(ht));
                let z = 4.0 * (1.0 + 0.25 * xts);
                p.line_segment([c + vec2(-z, -z), c + vec2(z, z)], st);
                p.line_segment([c + vec2(-z, z), c + vec2(z, -z)], st);
            }
        }
        hand(ui, &rs);
        hand(ui, &hs);
        hand(ui, &ps);
        let (like, add) = (hs.clicked(), ps.clicked());
        let rm = xs.as_ref().is_some_and(|x| x.clicked());
        let open = rs.clicked();
        hs.on_hover_text("");
        ps.on_hover_text("");
        if like {
            Some(Row::Like)
        } else if add {
            Some(Row::Add)
        } else if rm {
            Some(Row::Remove)
        } else if open {
            Some(Row::Open)
        } else {
            None
        }
    }

    pub(in crate::app) fn rows(
        &self,
        ui: &mut Ui,
        w: f32,
        items: &[Hit],
        removable: bool,
    ) -> Option<(Row, usize, Rect)> {
        let now = self.now_id();
        let mut out = None;
        for (i, h) in items.iter().enumerate() {
            let (r, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
            let r = Rect::from_min_size(r.min, vec2(w, 52.0));
            let on = now.as_deref() == Some(h.id.as_str());
            if let Some(a) = Self::hit_row(
                ui,
                r,
                h,
                i,
                on,
                &self.imgs,
                on && self.playing,
                self.liked_ids.contains(&h.id),
                removable,
            ) {
                out = Some((a, i, r));
            }
        }
        out
    }

    pub(in crate::app) fn pl_rows(&mut self, ui: &mut Ui, w: f32, pi: usize) -> Option<(Row, usize, Rect)> {
        const NONE: usize = usize::MAX;
        let pitch = 56.0_f32;
        let n = self.playlists[pi].tracks.len();
        let ctx = ui.ctx().clone();
        let (st_from, st_grab, st_to) = (Id::new("pl_ro_from"), Id::new("pl_ro_grab"), Id::new("pl_ro_to"));

        let base: Vec<Rect> = (0..n)
            .map(|_| {
                let (r, _) = ui.allocate_exact_size(vec2(w, pitch), Sense::hover());
                Rect::from_min_size(r.min, vec2(w, 52.0))
            })
            .collect();
        let mut from = ui.data(|d| d.get_temp::<usize>(st_from)).unwrap_or(NONE);
        let (down, decided, ptr) = ui.input(|i| {
            (
                i.pointer.any_down(),
                i.pointer.is_decidedly_dragging(),
                i.pointer.interact_pos(),
            )
        });

        let (st_spr, st_top) = (Id::new("pl_springs"), Id::new("pl_ro_top"));
        let mut spr: Vec<(f32, f32)> = ui.data(|d| d.get_temp::<Vec<(f32, f32)>>(st_spr)).unwrap_or_default();
        spr.resize(n, (0.0, 0.0));

        if from != NONE && (from >= n || !down) {
            if from < n {
                let to = ui.data(|d| d.get_temp::<usize>(st_to)).unwrap_or(from).min(n - 1);
                let top_last = ui.data(|d| d.get_temp::<f32>(st_top)).unwrap_or(base[from].top());
                if to != from {
                    let t = self.playlists[pi].tracks.remove(from);
                    self.playlists[pi].tracks.insert(to, t);
                    save_playlists(&self.playlists);
                    let mut ns = vec![(0.0_f32, 0.0_f32); n];
                    for j in 0..n {
                        let nj = if j == from {
                            to
                        } else if from < to && j > from && j <= to {
                            j - 1
                        } else if from > to && j >= to && j < from {
                            j + 1
                        } else {
                            j
                        };
                        ns[nj] = if j == from {
                            (top_last - base[nj].top(), 0.0)
                        } else {
                            (spr[j].0 - (nj as f32 - j as f32) * pitch, spr[j].1)
                        };
                    }
                    spr = ns;
                } else {
                    spr[from] = (top_last - base[from].top(), 0.0);
                }
            }
            ui.data_mut(|d| d.insert_temp(st_from, NONE));
            from = NONE;
        }

        if from == NONE && down && decided {
            let dr = ctx.dragged_id();
            if let Some(k) = (0..n).find(|&k| dr == Some(Id::new(("hit", k)))) {
                from = k;
                let grab = ptr.map_or(26.0, |p| p.y - base[k].top());
                spr[k] = (0.0, 0.0);
                ui.data_mut(|d| {
                    d.insert_temp(st_from, k);
                    d.insert_temp(st_grab, grab);
                    d.insert_temp(st_to, k);
                });
            }
        }

        let mut targets = vec![0.0_f32; n];
        let mut follow_top = 0.0_f32;
        if from != NONE && n > 0 {
            let grab = ui.data(|d| d.get_temp::<f32>(st_grab)).unwrap_or(26.0);
            let py = ptr.map_or(base[from].center().y, |p| p.y);
            let (first, last) = (base[0].top(), base[n - 1].top());
            follow_top = (py - grab).clamp(first, last);
            let to = (((follow_top + 26.0 - first) / pitch).floor().max(0.0) as usize).min(n - 1);
            ui.data_mut(|d| {
                d.insert_temp(st_to, to);
                d.insert_temp(st_top, follow_top);
            });
            for j in 0..n {
                targets[j] = if j == from {
                    0.0
                } else if from < to && j > from && j <= to {
                    -pitch
                } else if from > to && j >= to && j < from {
                    pitch
                } else {
                    0.0
                };
            }

            let (clip, edge) = (ui.clip_rect(), 56.0_f32);
            if py < clip.top() + edge {
                ui.scroll_with_delta(vec2(0.0, ((clip.top() + edge - py) / edge).clamp(0.0, 1.0) * 20.0));
            } else if py > clip.bottom() - edge {
                ui.scroll_with_delta(vec2(
                    0.0,
                    -((py - (clip.bottom() - edge)) / edge).clamp(0.0, 1.0) * 20.0,
                ));
            }
            ctx.set_cursor_icon(CursorIcon::Grabbing);
        }

        let dt = ui.input(|i| i.stable_dt).min(1.0 / 30.0);
        let (k_spring, c_damp, sub) = (420.0_f32, 17.0_f32, 4);
        let mut moving = from != NONE;
        for j in 0..n {
            if j == from {
                continue;
            }
            let (mut x, mut v) = spr[j];
            let h = dt / sub as f32;
            for _ in 0..sub {
                let acc = k_spring * (targets[j] - x) - c_damp * v;
                v += acc * h;
                x += v * h;
            }
            if (targets[j] - x).abs() < 0.15 && v.abs() < 2.0 {
                x = targets[j];
                v = 0.0;
            } else {
                moving = true;
            }
            spr[j] = (x, v);
        }
        ui.data_mut(|d| d.insert_temp(st_spr, spr.clone()));
        if moving {
            ctx.request_repaint();
        }

        let now = self.now_id();
        let tracks = &self.playlists[pi].tracks;
        let p = ui.painter().clone();
        let mut order: Vec<usize> = (0..n).filter(|&j| j != from).collect();
        order.sort_by(|&x, &y| {
            spr[x]
                .0
                .abs()
                .partial_cmp(&spr[y].0.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if from < n {
            order.push(from);
        }
        let mut out = None;
        for j in order {
            let h = &tracks[j];
            let held = j == from;
            let r = if held {
                Rect::from_min_size(pos2(base[j].left(), follow_top), base[j].size())
            } else {
                base[j].translate(vec2(0.0, spr[j].0))
            };
            if held || spr[j].0.abs() > 0.5 && from == NONE && spr[j].0.abs() > 20.0 {
                let a = if held {
                    18
                } else {
                    (18.0 * (spr[j].0.abs() / 56.0).min(1.0)) as u8
                };
                for sh in 1..=4 {
                    let e = sh as f32 * 2.0;
                    p.rect_filled(
                        r.expand(e).translate(vec2(0.0, 3.0)),
                        8.0 + e,
                        Color32::from_black_alpha(a),
                    );
                }
                p.rect_filled(r, 8.0, SURF2);
            }
            let on = now.as_deref() == Some(h.id.as_str());
            if let Some(a) = Self::hit_row(
                ui,
                r,
                h,
                j,
                on,
                &self.imgs,
                on && self.playing,
                self.liked_ids.contains(&h.id),
                true,
            ) {
                out = Some((a, j, r));
            }
        }
        out
    }

    pub(in crate::app) fn card(&mut self, ui: &mut Ui, r: Rect) {
        let Some(h) = self.sel.clone() else { return };
        let p = ui.painter().clone();
        ui.interact(r, Id::new("detail_block"), Sense::click());
        p.rect_filled(r, 10.0, SURF);

        let pad = 20.0;
        let iw = r.width() - 2.0 * pad;
        let x = r.left() + pad;
        let cs = (r.height() - 350.0).clamp(90.0, 280.0);
        let cr = Rect::from_min_size(pos2(r.left() + pad + (iw - cs) / 2.0, r.top() + pad), vec2(cs, cs));
        let big = h.thumb.replace("w96-h96", "w544-h544");
        art(ui, &p, cr, &self.imgs, &[&big, &h.thumb], seed_of(&h.id), 8.0);

        let cc = cr.right_top() + vec2(-20.0, 20.0);
        let crs = ui.interact(
            Rect::from_center_size(cc, vec2(28.0, 28.0)),
            Id::new("detail_close"),
            Sense::click(),
        );
        let xt = hov(ui, &crs);
        p.circle_filled(
            cc,
            13.0 + 1.5 * xt,
            Color32::from_black_alpha((140.0 + 60.0 * xt) as u8),
        );
        let st = Stroke::new(1.6_f32, TEXT);
        let xz = 4.0 * (1.0 + 0.2 * xt);
        p.line_segment([cc + vec2(-xz, -xz), cc + vec2(xz, xz)], st);
        p.line_segment([cc + vec2(-xz, xz), cc + vec2(xz, -xz)], st);
        hand(ui, &crs);

        let mut y = cr.bottom() + 16.0;
        spaced(
            &p,
            pos2(x, y + 6.0),
            Align2::LEFT_CENTER,
            tr(self.language, "selection"),
            11.0,
            accent(),
        );
        y += 20.0;
        let th = wrapped(&p, pos2(x, y), &h.title, FontId::proportional(20.0), TEXT, iw, 2);
        y += th + 2.0;
        fit(&p, pos2(x, y + 8.0), &h.artist, FontId::proportional(14.0), MUTED, iw);
        y += 26.0;

        let cur = self.now_id().as_deref() == Some(h.id.as_str());
        let (list, idx) = self.card_list(&h);
        let secs = if cur { self.now_secs() } else { h.secs as f32 };
        let mut prog = if cur { self.progress } else { 0.0 };
        let wr = Rect::from_min_size(pos2(x, y + 4.0), vec2(iw, 24.0));
        let (drag, commit) = wave_id(ui, wr, &mut prog, seed_of(&h.id) % 97, "card_seek", secs);
        if cur {
            self.card_drag = drag;
            self.progress = prog;
            if commit {
                self.audio.seek(prog * secs);
            }
        } else if commit {
            self.play_list(list.clone(), idx);
            self.progress = prog;
        }
        p.text(
            pos2(x, y + 42.0),
            Align2::LEFT_CENTER,
            mmss(prog * secs),
            FontId::proportional(11.0),
            MUTED,
        );
        p.text(
            pos2(x + iw, y + 42.0),
            Align2::RIGHT_CENTER,
            mmss(secs),
            FontId::proportional(11.0),
            MUTED,
        );
        y += 56.0;

        let (cx, cy) = (r.center().x, y + 20.0);
        let mut ctl: Option<u8> = None;
        if ibtn(ui, Ic::Shuffle, pos2(cx - 96.0, cy), 15.0, self.shuffle, "cd_shuf") {
            self.shuffle = !self.shuffle;
            self.shuf_next = None;
        }
        if ibtn(ui, Ic::Prev, pos2(cx - 48.0, cy), 15.0, false, "cd_prev") {
            ctl = Some(0);
        }
        let pb = Rect::from_center_size(pos2(cx, cy), vec2(44.0, 44.0));
        let prs = ui.interact(pb, Id::new("cd_play"), Sense::click());
        let pz = pop(ui, &prs, pb, 3.0);
        p.rect_filled(pz, 8.0, accent());
        icon(
            &p,
            if cur && self.playing { Ic::Pause } else { Ic::Play },
            pb.center(),
            16.0 * pz.width() / pb.width(),
            on_accent(),
        );
        hand(ui, &prs);
        if prs.clicked() {
            ctl = Some(1);
        }
        if ibtn(ui, Ic::Next, pos2(cx + 48.0, cy), 15.0, false, "cd_next") {
            ctl = Some(2);
        }
        if ibtn(ui, Ic::Repeat, pos2(cx + 96.0, cy), 15.0, self.repeat, "cd_rep") {
            self.repeat = !self.repeat;
            self.shuf_next = None;
        }
        y += 56.0;
        match ctl {
            Some(1) => {
                if cur {
                    self.playing = !self.playing;
                } else {
                    self.play_list(list.clone(), idx);
                }
            }
            Some(k) => {
                let d: isize = if k == 0 { -1 } else { 1 };
                if cur {
                    self.step(d);
                    if let Some(n) = self.queue.get(self.qi).cloned() {
                        self.sel = Some(n);
                    }
                } else if !list.is_empty() {
                    let j = (idx as isize + d).rem_euclid(list.len() as isize) as usize;
                    let n = list[j].clone();
                    self.play_list(list.clone(), j);
                    self.sel = Some(n);
                }
            }
            None => {}
        }

        p.line_segment([pos2(x, y), pos2(x + iw, y)], Stroke::new(1.0_f32, LINE));
        y += 12.0;

        if !h.album.is_empty() && h.album != h.title {
            spaced(
                &p,
                pos2(x, y + 14.0),
                Align2::LEFT_CENTER,
                tr(self.language, "album"),
                10.0,
                MUTED,
            );
            fit(
                &p,
                pos2(x + 72.0, y + 14.0),
                &h.album,
                FontId::proportional(13.0),
                TEXT,
                iw - 72.0,
            );
            y += 28.0;
        }

        let now = ui.input(|i| i.time);
        let liked = self.liked_ids.contains(&h.id);
        let like_lab = if liked {
            tr(self.language, "remove_liked")
        } else {
            tr(self.language, "like")
        };
        let actions = [
            (Ic::Heart(liked), like_lab),
            (Ic::Plus, tr(self.language, "add_playlist")),
            (Ic::Queue, tr(self.language, "play_next")),
            (Ic::Share, tr(self.language, "share")),
        ];
        let mut act: Option<(usize, Pos2)> = None;
        for (k, (ic, lab)) in actions.into_iter().enumerate() {
            let b = Rect::from_min_size(pos2(x + k as f32 * 44.0, y), vec2(40.0, 40.0));
            let rs = ui.interact(b, Id::new(("act", k)), Sense::click());
            let t = hov(ui, &rs);
            if t > 0.003 {
                p.rect_filled(b.expand(1.5 * t), 8.0, SURF2.gamma_multiply(t));
            }
            let col = if k == 0 && liked {
                accent()
            } else {
                MUTED.lerp_to_gamma(TEXT, t)
            };
            icon(&p, ic, b.center(), 17.0 + 3.0 * t, col);
            hand(ui, &rs);
            if rs.clicked() {
                act = Some((k, b.center()));
            }
            rs.on_hover_text(lab);
        }
        match act {
            Some((0, _)) => self.toggle_like_hit(&h),
            Some((1, at)) => self.pl_menu = Some(PlMenu { hit: h.clone(), at }),
            Some((2, _)) => {
                let msg = self.queue_next(&h);
                self.toast = Some((msg.to_string(), now));
            }
            Some((3, _)) => {
                ui.ctx()
                    .copy_text(format!("https://music.youtube.com/watch?v={}", h.id));
                self.toast = Some((tr(self.language, "link_copied").to_string(), now));
            }
            _ => {}
        }
        if let Some((t, t0)) = self.toast.clone() {
            if now - t0 < 2.0 {
                p.text(
                    pos2(x + iw, y + 20.0),
                    Align2::RIGHT_CENTER,
                    t,
                    FontId::proportional(11.0),
                    accent(),
                );
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            } else {
                self.toast = None;
            }
        }

        if crs.clicked() {
            self.sel = None;
        }
    }
}
