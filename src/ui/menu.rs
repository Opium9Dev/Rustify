use super::super::*;

impl super::super::App {
    pub(in crate::app) fn root(&mut self, ui: &mut Ui) {
        if self.mini {
            if self.exit_anim.is_some() {
                self.exit_view(ui);
                return;
            }
            self.mini_view(ui);
            let win = ui.max_rect();
            self.anim_overlay(ui, win, WIN_R);
            if self.fade_a > 0.003 {
                ui.painter()
                    .rect_stroke(win.shrink(0.5), (WIN_R - 0.5).max(0.0), Stroke::new(1.0_f32, LINE));
            }
            return;
        }
        let win = ui.max_rect();
        let maximized = ui.ctx().input(|i| i.viewport().maximized).unwrap_or(false);
        let rad = if maximized { 0.0 } else { WIN_R };
        ui.painter().rect_filled(win, (rad - 2.0).max(0.0), window_bg());
        let full = Rect::from_min_max(pos2(win.left(), win.top() + TB_H), win.max);
        self.title_bar(ui, Rect::from_min_max(win.min, pos2(win.right(), win.top() + TB_H)));

        if matches!(self.pui, Pui::Picker | Pui::Form) {
            self.profile_screen(ui, full);
            self.dep_dialog(ui, full);
            self.sw_overlay(ui, full, rad);
            if !maximized && !self.animating() {
                Self::resize_handles(ui, win);
            }
            if !maximized {
                ui.painter()
                    .rect_stroke(win.shrink(0.5), (rad - 0.5).max(0.0), Stroke::new(1.0_f32, LINE));
            }
            return;
        }
        let rail = Rect::from_min_max(
            pos2(full.left() + 12.0, full.top() + 12.0),
            pos2(full.left() + 72.0, full.bottom() - 12.0),
        );
        let main = Rect::from_min_max(
            pos2(rail.right() + 24.0, full.top() + 8.0),
            pos2(full.right() - 4.0, full.bottom()),
        );
        let pw = (main.width() - 32.0).min(800.0);
        let pill = Rect::from_center_size(
            pos2(main.left() + (main.width() - 32.0) / 2.0, full.bottom() - 41.0),
            vec2(pw, 54.0),
        );

        let show_card = self.view == View::Search && self.sel.is_some();
        let card = Rect::from_min_max(
            pos2(full.right() - 340.0, full.top() + 20.0),
            pos2(full.right() - 20.0, pill.top() - 24.0),
        );
        let main_v = if show_card {
            Rect::from_min_max(main.min, pos2(card.left() - 8.0, main.bottom()))
        } else {
            main
        };

        self.rail(ui, rail);
        self.main_view(ui, main_v);

        self.card_drag = false;
        if show_card {
            self.card(ui, card);
        }
        self.player(ui, pill);
        self.playlist_menu(ui, full);
        self.playlist_ctx_menu(ui, full);
        self.profile_panel(ui, full, rail);

        let mb = Rect::from_center_size(pos2(full.right() - 27.0, full.bottom() - 27.0), vec2(34.0, 34.0));
        let mrs = ui.interact(mb, Id::new("mini_btn"), Sense::click());
        {
            let p = ui.painter().clone();
            let t = hov(ui, &mrs);
            if t > 0.003 {
                p.rect_filled(mb.expand(1.5 * t), 9.0, SURF2.gamma_multiply(t));
            }
            let col = MUTED.lerp_to_gamma(TEXT, t);
            let c = mb.center();
            let z = 1.0 + 0.15 * t;
            p.rect_stroke(
                Rect::from_center_size(c, vec2(19.0, 14.0) * z),
                2.5,
                Stroke::new(1.6_f32, col),
            );
            p.rect_filled(
                Rect::from_min_max(c + vec2(0.5, 0.5) * z, c + vec2(7.0, 4.5) * z),
                1.5,
                col,
            );
        }
        hand(ui, &mrs);
        let mrs = mrs.on_hover_text(tr(self.language, "mini_player"));
        if mrs.clicked() {
            let ctx = ui.ctx().clone();
            self.enter_mini(&ctx);
        }

        self.dep_dialog(ui, full);
        if !maximized && !self.animating() {
            Self::resize_handles(ui, win);
        }
        self.sw_overlay(ui, full, rad);
        self.anim_overlay(ui, win, rad);

        if !maximized {
            ui.painter()
                .rect_stroke(win.shrink(0.5), (rad - 0.5).max(0.0), Stroke::new(1.0_f32, LINE));
        }
    }

    pub(in crate::app) fn rail(&mut self, ui: &mut Ui, r: Rect) {
        let p = ui.painter().clone();
        p.rect_filled(r, 10.0, SURF);
        let lc = pos2(r.center().x, r.top() + 36.0);
        for (k, hh) in [11.0f32, 22.0, 15.0].iter().enumerate() {
            p.rect_filled(
                Rect::from_center_size(lc + vec2((k as f32 - 1.0) * 7.0, 0.0), vec2(4.0, *hh)),
                1.0,
                accent(),
            );
        }
        let items = [
            (tr(self.language, "home"), View::Home),
            (tr(self.language, "search"), View::Search),
            (tr(self.language, "library"), View::Library),
        ];

        let cur_i = items.iter().position(|(_, v)| *v == self.view).unwrap_or(0);
        let target = r.top() + 100.0 + cur_i as f32 * 50.0;
        let dt = ui.input(|i| i.stable_dt);
        let iy = self.nav_y.get_or_insert(target);
        if (target - *iy).abs() > 0.3 {
            *iy += (target - *iy) * (1.0 - (-dt * 16.0).exp());
            ui.ctx().request_repaint();
        } else {
            *iy = target;
        }
        let iy = *iy;
        p.rect_filled(
            Rect::from_center_size(pos2(r.center().x, iy), vec2(40.0, 40.0)),
            8.0,
            accent(),
        );

        for (i, (lab, v)) in items.into_iter().enumerate() {
            let ctr = pos2(r.center().x, r.top() + 100.0 + i as f32 * 50.0);
            let br = Rect::from_center_size(ctr, vec2(40.0, 40.0));
            let rs = ui.interact(br, Id::new(("nav", i)), Sense::click());

            let t = (1.0 - (iy - ctr.y).abs() / 40.0).clamp(0.0, 1.0);
            let on = t > 0.5;
            let ht = hov(ui, &rs);
            if ht > 0.003 {
                p.rect_filled(br.expand(1.5 * ht), 8.0, SURF2.gamma_multiply(ht * (1.0 - t)));
            }
            let ic = match v {
                View::Home => Ic::Home(on),
                View::Search => Ic::Search,
                View::Library => Ic::Library,
            };
            let base = MUTED.lerp_to_gamma(TEXT, ht);
            icon(&p, ic, ctr, 17.0 + 3.0 * ht, base.lerp_to_gamma(on_accent(), t));
            hand(ui, &rs);
            if rs.clicked() {
                self.view = v;
            }
            rs.on_hover_text(lab);
        }

        let sb = Rect::from_center_size(pos2(r.center().x, r.bottom() - 32.0), vec2(40.0, 40.0));
        let srs = ui.interact(sb, Id::new("profile_btn"), Sense::click());
        let st = hov(ui, &srs);
        let ctx = ui.ctx().clone();
        let me = self.cur_profile();
        let tex = me.as_ref().and_then(|pr| self.photo_tex(&ctx, pr));
        let open = self.pui == Pui::Panel;
        let ring = st.max(hov_if(ui, Id::new("pf_open"), open));
        if ring > 0.003 {
            p.circle_stroke(
                sb.center(),
                20.0 + 1.5 * st,
                Stroke::new(1.5_f32, accent().gamma_multiply(ring)),
            );
        }
        draw_avatar(
            &p,
            sb.center(),
            16.0 + 1.5 * st,
            me.as_ref().map(|pr| pr.avatar),
            tex.as_ref(),
        );
        hand(ui, &srs);
        let tip = me
            .as_ref()
            .map(|pr| pr.name.clone())
            .unwrap_or_else(|| tr(self.language, "guest").to_string());
        let srs = srs.on_hover_text(tip);
        if srs.clicked() {
            self.pui = if open { Pui::Off } else { Pui::Panel };
        }
    }

    pub(in crate::app) fn main_view(&mut self, ui: &mut Ui, r: Rect) {
        ui.allocate_new_ui(UiBuilder::new().max_rect(r), |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .drag_to_scroll(false)
                .scroll_bar_visibility(scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    if !ui.ctx().wants_keyboard_input() {
                        let dy = ui.input(|i| {
                            let mut d = 0.0;
                            if i.key_pressed(Key::ArrowDown) {
                                d -= 70.0;
                            }
                            if i.key_pressed(Key::ArrowUp) {
                                d += 70.0;
                            }
                            if i.key_pressed(Key::PageDown) {
                                d -= 500.0;
                            }
                            if i.key_pressed(Key::PageUp) {
                                d += 500.0;
                            }
                            d
                        });
                        if dy != 0.0 {
                            ui.scroll_with_delta(vec2(0.0, dy));
                        }
                    }
                    let w = ui.available_width() - 32.0;
                    ui.add_space(12.0);
                    match self.view {
                        View::Home => self.home(ui, w),
                        View::Search => self.search(ui, w),
                        View::Library => self.library(ui, w),
                    }
                    ui.add_space(110.0);
                });
        });
    }
}
