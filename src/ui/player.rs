use super::super::*;

impl super::super::App {
    pub(in crate::app) fn player(&mut self, ui: &mut Ui, r: Rect) {
        ui.interact(r, Id::new("pill_block"), Sense::click());
        let p = ui.painter().clone();

        for k in 1..=7 {
            let e = k as f32 * 2.0;
            p.rect_filled(
                r.expand(e).translate(vec2(0.0, 3.0)),
                10.0 + e,
                Color32::from_black_alpha(9),
            );
        }
        p.rect_filled(r, 10.0, MENU);
        p.rect_stroke(r.shrink(0.5), 9.5, Stroke::new(1.0_f32, HAIR));
        let h = self.now();
        let seed = self.now_seed();
        let (x0, x1, cy) = (r.left(), r.right(), r.center().y);

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
        let cov = Rect::from_center_size(pos2(x0 + 30.0, cy), vec2(38.0, 38.0));
        if self.started {
            art(ui, &p, cov, &self.imgs, &[&h.thumb], seed, 5.0);
        } else {
            p.rect_filled(cov, 5.0, SURF);
        }
        let tclip = Rect::from_min_max(pos2(x0 + 58.0, r.top()), pos2(x0 + 178.0, r.bottom()));
        marquee(ui, &p, tclip, cy - 7.0, &title, FontId::proportional(13.0), TEXT);
        marquee(ui, &p, tclip, cy + 8.0, &artist, FontId::proportional(11.0), MUTED);
        if let Some(e) = self.audio.err.clone() {
            let tr = Rect::from_min_max(pos2(x0 + 58.0, r.top()), pos2(x0 + 178.0, r.bottom()));
            ui.interact(tr, Id::new("pl_err"), Sense::hover()).on_hover_text(e);
        }
        let liked = self.is_liked();
        if ibtn(ui, Ic::Heart(liked), pos2(x0 + 196.0, cy), 13.0, liked, "pl_like") {
            self.toggle_like();
        }
        if ibtn(ui, Ic::Plus, pos2(x0 + 226.0, cy), 14.0, false, "pl_add") && self.started {
            self.pl_menu = Some(PlMenu {
                hit: h.clone(),
                at: pos2(x0 + 226.0, cy),
            });
        }

        if ibtn(ui, Ic::Shuffle, pos2(x0 + 262.0, cy), 13.0, self.shuffle, "shuf") {
            self.shuffle = !self.shuffle;
        }
        if ibtn(ui, Ic::Prev, pos2(x0 + 294.0, cy), 13.0, false, "prev") {
            self.step(-1);
        }
        let pb = Rect::from_center_size(pos2(x0 + 330.0, cy), vec2(34.0, 34.0));
        let prs = ui.interact(pb, Id::new("playbtn"), Sense::click());
        let pz = pop(ui, &prs, pb, 3.0);
        p.rect_filled(pz, 7.0, accent());
        icon(
            &p,
            if self.playing { Ic::Pause } else { Ic::Play },
            pb.center(),
            12.0 * pz.width() / pb.width(),
            on_accent(),
        );
        hand(ui, &prs);
        if prs.clicked() {
            self.toggle_play();
        }
        if ibtn(ui, Ic::Next, pos2(x0 + 366.0, cy), 13.0, false, "next") {
            self.step(1);
        }
        if ibtn(ui, Ic::Repeat, pos2(x0 + 398.0, cy), 13.0, self.repeat, "rep") {
            self.repeat = !self.repeat;
        }

        let secs = if self.started { self.now_secs() } else { 0.0 };
        p.text(
            pos2(x0 + 422.0, cy),
            Align2::LEFT_CENTER,
            mmss(self.progress * secs),
            FontId::proportional(10.0),
            MUTED,
        );

        let compact = r.width() < 760.0;
        let wr = Rect::from_min_max(
            pos2(x0 + 456.0, cy - 10.0),
            pos2(x1 - if compact { 70.0 } else { 150.0 }, cy + 10.0),
        );
        if self.started {
            let (drag, commit) = wave(ui, wr, &mut self.progress, seed % 97, secs);
            self.seeking = drag || self.card_drag;
            if commit {
                self.audio.seek(self.progress * secs);
            }
        } else {
            wave_bars(&p, wr, 0.0, 0);
        }
        p.text(
            pos2(wr.right() + 8.0, cy),
            Align2::LEFT_CENTER,
            mmss(secs),
            FontId::proportional(10.0),
            MUTED,
        );

        if !compact {
            icon(&p, Ic::Vol, pos2(x1 - 90.0, cy), 13.0, MUTED);
            slider(
                ui,
                Rect::from_min_size(pos2(x1 - 76.0, cy - 2.0), vec2(56.0, 4.0)),
                &mut self.volume,
                "vol",
            );
        }
    }
}
