//! M3: ランチャー画面 (SPEC 6.5)
//!
//! アイコン横並びグリッド。Mica/Acrylic + 角丸 + ダーク/ライト追従 +
//! ホバーアニメーション。マウスカーソル付近にポップアップする。

mod window;
use window::{apply_window_effects, popup_position};

use std::sync::mpsc;

use anyhow::{anyhow, Result};
use eframe::egui::{self, Color32, CornerRadius, FontId, Id, Margin, Pos2, Rect, RichText, Sense, Stroke, Vec2};

use crate::{icon, platform};

const ICON_SIZE: i32 = 64;
const TILE_W: f32 = 116.0;
const TILE_H: f32 = 140.0;
const TILE_GAP: f32 = 12.0;
const OUTER_MARGIN: f32 = 18.0;
const MAX_COLS: usize = 6;

pub struct Candidate {
    /// 設定上のアプリ名 (例: "vscode")
    pub name: String,
    /// 補足ラベル (例: "Profile 1")
    pub label: Option<String>,
    /// 展開済みの実行ファイルパス (アイコン抽出に使う)
    pub program: String,
}

/// ピッカーを表示し、選ばれたアプリ名を返す (None = キャンセル)
pub fn show(target_label: String, candidates: Vec<Candidate>, timeout_ms: u64) -> Result<Option<String>> {
    if candidates.is_empty() {
        return Ok(None);
    }
    platform::init_com();
    let images: Vec<Option<egui::ColorImage>> = candidates
        .iter()
        .map(|c| icon::extract_icon_rgba(&c.program, ICON_SIZE))
        .collect();

    let cols = candidates.len().min(MAX_COLS);
    let rows = candidates.len().div_ceil(MAX_COLS);
    let width = (OUTER_MARGIN * 2.0 + cols as f32 * TILE_W + (cols - 1) as f32 * TILE_GAP).max(300.0);
    let height = OUTER_MARGIN * 2.0 + 60.0 + rows as f32 * TILE_H + (rows - 1) as f32 * TILE_GAP;
    let position = popup_position(width, height);

    let dark = platform::prefers_dark_theme();
    let (tx, rx) = mpsc::channel::<Option<String>>();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_position(position)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_taskbar(false),
        centered: false,
        ..Default::default()
    };

    eframe::run_native(
        "winassoc",
        options,
        Box::new(move |cc| {
            Ok(Box::new(PickerApp::new(cc, target_label, candidates, images, dark, tx, timeout_ms)))
        }),
    )
    .map_err(|e| anyhow!("ピッカーの起動に失敗しました: {e}"))?;

    Ok(rx.try_recv().ok().flatten())
}

struct PickerApp {
    target_label: String,
    candidates: Vec<Candidate>,
    textures: Vec<Option<egui::TextureHandle>>,
    selected: usize,
    dark: bool,
    tx: mpsc::Sender<Option<String>>,
    sent: bool,
    has_focused: bool,
    start_time: std::time::Instant,
    last_focused_time: std::time::Instant,
    timeout_ms: u64,
}

impl PickerApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        target_label: String,
        candidates: Vec<Candidate>,
        images: Vec<Option<egui::ColorImage>>,
        dark: bool,
        tx: mpsc::Sender<Option<String>>,
        timeout_ms: u64,
    ) -> Self {
        install_japanese_fonts(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(if dark { egui::Visuals::dark() } else { egui::Visuals::light() });
        apply_window_effects(cc, dark);

        let textures = images
            .into_iter()
            .enumerate()
            .map(|(i, img)| {
                img.map(|img| {
                    cc.egui_ctx.load_texture(format!("icon-{i}"), img, egui::TextureOptions::LINEAR)
                })
            })
            .collect();

        let now = std::time::Instant::now();
        Self {
            target_label,
            candidates,
            textures,
            selected: 0,
            dark,
            tx,
            sent: false,
            has_focused: false,
            start_time: now,
            last_focused_time: now,
            timeout_ms,
        }
    }

    fn finish(&mut self, ctx: &egui::Context, choice: Option<usize>) {
        if !self.sent {
            self.sent = true;
            let _ = self.tx.send(choice.map(|i| self.candidates[i].name.clone()));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let mut choose: Option<usize> = None;
        let mut cancel = false;
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                cancel = true;
            }
            if i.key_pressed(egui::Key::Enter) {
                choose = Some(self.selected);
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                self.selected = (self.selected + 1) % self.candidates.len();
            }
            if i.key_pressed(egui::Key::ArrowLeft) {
                self.selected = (self.selected + self.candidates.len() - 1) % self.candidates.len();
            }
            const DIGITS: [egui::Key; 9] = [
                egui::Key::Num1, egui::Key::Num2, egui::Key::Num3,
                egui::Key::Num4, egui::Key::Num5, egui::Key::Num6,
                egui::Key::Num7, egui::Key::Num8, egui::Key::Num9,
            ];
            for (n, key) in DIGITS.iter().enumerate() {
                if n < self.candidates.len() && i.key_pressed(*key) {
                    choose = Some(n);
                }
            }
        });
        if cancel {
            self.finish(ctx, None);
        } else if let Some(i) = choose {
            self.finish(ctx, Some(i));
        }
    }

    fn draw_tile(&mut self, ui: &mut egui::Ui, index: usize) -> Option<usize> {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(TILE_W, TILE_H), Sense::click());
        let mut chosen = None;
        if response.hovered() {
            self.selected = index;
        }
        if response.clicked() {
            chosen = Some(index);
        }

        let selected = self.selected == index;
        // ホバー/選択アニメーション (SPEC 6.5: ≦150ms のイージング)
        let t = ui.ctx().animate_bool_with_time(Id::new(("tile", index)), selected, 0.12);

        let accent = if self.dark {
            Color32::from_rgb(96, 165, 250)
        } else {
            Color32::from_rgb(37, 99, 235)
        };
        let base_fill = if self.dark {
            Color32::from_rgba_unmultiplied(255, 255, 255, 10)
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 8)
        };
        let hover_fill = if self.dark {
            Color32::from_rgba_unmultiplied(255, 255, 255, 32)
        } else {
            Color32::from_rgba_unmultiplied(0, 0, 0, 18)
        };

        let painter = ui.painter();
        let fill = lerp_color(base_fill, hover_fill, t);
        painter.rect_filled(rect, CornerRadius::same(12), fill);
        if t > 0.01 {
            painter.rect_stroke(
                rect.shrink(0.5),
                CornerRadius::same(12),
                Stroke::new(1.5 * t, accent),
                egui::StrokeKind::Inside,
            );
        }

        // アイコン (選択時 8% 拡大)
        let icon_size = 56.0 * (1.0 + 0.08 * t);
        let icon_center = Pos2::new(rect.center().x, rect.top() + 38.0);
        let icon_rect = Rect::from_center_size(icon_center, Vec2::splat(icon_size));
        match &self.textures[index] {
            Some(texture) => {
                painter.image(
                    texture.id(),
                    icon_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                // アイコン抽出失敗時: 頭文字のジェネリックタイル (SPEC 6.5)
                painter.rect_filled(icon_rect, CornerRadius::same(10), accent.gamma_multiply(0.25));
                let initial = self.candidates[index]
                    .name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default();
                painter.text(
                    icon_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    initial,
                    FontId::proportional(28.0),
                    accent,
                );
            }
        }

        let text_color = if self.dark { Color32::from_gray(235) } else { Color32::from_gray(25) };
        let subtle = if self.dark { Color32::from_gray(160) } else { Color32::from_gray(110) };
        painter.text(
            Pos2::new(rect.center().x, rect.top() + 82.0),
            egui::Align2::CENTER_TOP,
            &self.candidates[index].name,
            FontId::proportional(16.0),
            text_color,
        );
        if let Some(label) = &self.candidates[index].label {
            painter.text(
                Pos2::new(rect.center().x, rect.top() + 104.0),
                egui::Align2::CENTER_TOP,
                label,
                FontId::proportional(11.5),
                subtle,
            );
        }
        if index < 9 {
            let keycap_center = Pos2::new(rect.center().x, rect.bottom() - 13.0);
            let keycap_rect = Rect::from_center_size(keycap_center, Vec2::new(20.0, 16.0));
            
            let keycap_fill = if self.dark {
                Color32::from_rgba_unmultiplied(255, 255, 255, 20)
            } else {
                Color32::from_rgba_unmultiplied(0, 0, 0, 15)
            };
            let keycap_stroke = if self.dark {
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 45))
            } else {
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 30))
            };
            
            painter.rect_filled(keycap_rect, CornerRadius::same(4), keycap_fill);
            painter.rect_stroke(
                keycap_rect,
                CornerRadius::same(4),
                keycap_stroke,
                egui::StrokeKind::Inside,
            );
            
            painter.text(
                Pos2::new(keycap_center.x, keycap_center.y + 2.0), // 視覚的な位置調整
                egui::Align2::CENTER_CENTER,
                format!("{}", index + 1),
                FontId::proportional(11.0),
                subtle,
            );
        }
        chosen
    }
}

impl eframe::App for PickerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_keys(ctx);

        // アクティブ判定：OSウィンドウフォーカスがある、またはマウスがウィンドウ内にある
        let focused = ctx.input(|i| i.focused);
        let pointer_over = ctx.input(|i| i.pointer.hover_pos().is_some());
        let active = focused || pointer_over;

        if active {
            self.has_focused = true;
            self.last_focused_time = std::time::Instant::now();
        }

        // フォーカスを失ったら閉じる (SPEC 6.5)。起動直後の未フォーカスは無視。
        // 一時的なフォーカス喪失やチラつきによる誤終了を防ぐため、300msのデバウンスを設ける
        let should_close = if self.has_focused {
            !active && self.last_focused_time.elapsed().as_millis() > 300
        } else {
            !active && self.start_time.elapsed().as_millis() > self.timeout_ms as u128
        };

        if should_close {
            self.finish(ctx, None);
        }

        // Mica/Acrylic を透かすため半透明の塗りにする。境界線も追加してデザインを際立たせる
        let panel_fill = if self.dark {
            Color32::from_rgba_unmultiplied(24, 26, 32, 215)
        } else {
            Color32::from_rgba_unmultiplied(248, 249, 252, 225)
        };
        let panel_stroke = if self.dark {
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20))
        } else {
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 15))
        };
        let subtle = if self.dark { Color32::from_gray(160) } else { Color32::from_gray(110) };

        let mut chosen: Option<usize> = None;
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(panel_fill)
                    .stroke(panel_stroke)
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(OUTER_MARGIN as i8)),
            )
            .show(ctx, |ui| {
                // 自動挿入されるデフォルトの縦スペースを無効化し、高さを完全にコントロールする
                ui.spacing_mut().item_spacing.y = 0.0;

                ui.label(
                    RichText::new(format!("{} を開くアプリを選択", self.target_label))
                        .size(15.0)
                        .color(subtle),
                );
                ui.add_space(12.0);

                let total = self.candidates.len();
                let rows = total.div_ceil(MAX_COLS);
                for row in 0..rows {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(TILE_GAP, TILE_GAP);
                        for index in (row * MAX_COLS)..((row + 1) * MAX_COLS).min(total) {
                            if let Some(i) = self.draw_tile(ui, index) {
                                chosen = Some(i);
                            }
                        }
                    });
                    if row + 1 < rows {
                        ui.add_space(TILE_GAP);
                    }
                }
                ui.add_space(12.0);

                ui.label(
                    RichText::new("←/→ 選択 · Enter/数字で決定 · Esc で中止")
                        .size(11.5)
                        .color(subtle),
                );
            });

        if let Some(i) = chosen {
            self.finish(ctx, Some(i));
        }
        // アニメーション中は再描画を続ける
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }
}

// ───────────────────────── フォント ─────────────────────────

/// 既定フォントに CJK グリフがないため、システムの日本語フォントを追加する
fn install_japanese_fonts(ctx: &egui::Context) {
    const CANDIDATE_FONTS: [&str; 4] = [
        r"C:\Windows\Fonts\YuGothM.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        r"C:\Windows\Fonts\msgothic.ttc",
        r"C:\Windows\Fonts\segoeui.ttf",
    ];
    let Some(bytes) = CANDIDATE_FONTS.iter().find_map(|p| std::fs::read(p).ok()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("jp".to_string(), egui::FontData::from_owned(bytes).into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().insert(0, "jp".to_string());
    }
    ctx.set_fonts(fonts);
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_unmultiplied(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
        lerp(a.a(), b.a()),
    )
}
