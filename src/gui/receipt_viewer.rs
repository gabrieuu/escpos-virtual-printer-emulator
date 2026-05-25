use crate::emulator::EmulatorState;
use crate::escpos::printer::{PrinterState, ReceiptLine};
use egui::{ColorImage, ScrollArea, TextureHandle, TextureOptions, Ui};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::escpos::commands::Justification;

pub struct ReceiptViewer {
    show_paper_edges: bool,
    show_grid: bool,
    /// Cache of rendered bitmap textures (keyed by data hash)
    bitmap_cache: HashMap<u64, TextureHandle>,
}

impl Default for ReceiptViewer {
    fn default() -> Self {
        Self {
            show_paper_edges: true,
            show_grid: false,
            bitmap_cache: HashMap::new(),
        }
    }
}

fn hash_bytes(data: &[u8]) -> u64 {
    // Simple FNV-1a hash for cache key
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

impl ReceiptViewer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self, ui: &mut Ui, emulator_state: &Arc<Mutex<EmulatorState>>) {
        ui.heading("🖨️ Receipt Viewer");
        ui.separator();

        // Controls
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_paper_edges, "Show paper edges");
            ui.checkbox(&mut self.show_grid, "Show grid");

            if ui.button("🗑️ Clear").clicked() {
                if let Ok(mut state) = emulator_state.try_lock() {
                    state.clear_printer_buffer();
                }
                self.bitmap_cache.clear();
            }
        });

        ui.separator();

        // Receipt display area
        ScrollArea::both().show(ui, |ui| {
            if let Ok(state) = emulator_state.try_lock() {
                self.render_receipt(ui, &state);
            } else {
                ui.label("Cannot load emulator state");
            }
        });
    }

    fn render_receipt(&mut self, ui: &mut Ui, state: &EmulatorState) {
        let printer_state = state.get_printer_state();
        let buffer = printer_state.get_buffer();

        if buffer.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("No receipt data available");
                ui.label("Send ESC/POS commands to see the receipt here");
            });
            return;
        }

        // Paper simulation
        let paper_width = printer_state.get_paper_width_dots();
        let max_chars = printer_state.paper_width.get_max_chars(printer_state.font_size);

        ui.group(|ui| {
            // Paper header
            ui.horizontal(|ui| {
                ui.label(format!("📄 Paper: {:?}", printer_state.paper_width));
                ui.label(format!("🔤 Font: {:?}", printer_state.current_font));
                ui.label(format!("📐 Align: {:?}", printer_state.justification));
                if printer_state.codepage != 0 {
                    ui.label(format!("🌐 CP: {}", printer_state.codepage));
                }
            });

            ui.separator();

            // Receipt content
            for (line_num, line) in buffer.iter().enumerate() {
                match line {
                    ReceiptLine::Text(text, style) => {
                        if !text.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{:03}", line_num + 1));
                                ui.label("│");

                                // font_size do ESC/POS: 0=normal, 16=double-width, 32=double-height, 48=double
                                let scale = if style.font_size >= 16 { 2.0f32 } else { 1.0f32 };
                                let max_chars = (printer_state.paper_width
                                    .get_max_chars(12) as f32 / scale) as usize;

                                let aligned_text = match style.justification {
                                    Justification::Center => {
                                        let padding = max_chars.saturating_sub(text.chars().count()) / 2;
                                        format!("{}{}", " ".repeat(padding), text)
                                    }
                                    Justification::Right => {
                                        let padding = max_chars.saturating_sub(text.chars().count());
                                        format!("{}{}", " ".repeat(padding), text)
                                    }
                                    _ => text.clone(),
                                };

                                // Aplicar tamanho de fonte visual
                                let font_size = if style.font_size >= 16 { 18.0 } else { 13.0 };
                                let text_widget = egui::RichText::new(&aligned_text)
                                    .monospace()
                                    .size(font_size);

                                let text_widget = if style.emphasis {
                                    text_widget.strong()
                                } else {
                                    text_widget
                                };

                                let text_widget = if style.underline {
                                    text_widget.underline()
                                } else {
                                    text_widget
                                };

                                ui.label(text_widget);
                            });
                        }
                    }
                    ReceiptLine::Bitmap { width_px, height_px, data } => {
                        self.render_bitmap(ui, *width_px, *height_px, data, paper_width);
                    }
                    ReceiptLine::Separator => {
                        let sep = "─".repeat(max_chars as usize);
                        ui.horizontal(|ui| {
                            ui.label(format!("{:03}", line_num + 1));
                            ui.label("│");
                            ui.label(&sep);
                        });
                    }
                }
            }

            // Paper footer
            ui.separator();
            ui.label("✂️ Cut line");
        });
    }

    fn render_bitmap(&mut self, ui: &mut Ui, width_px: u32, height_px: u32, data: &[u8], _paper_width: u32) {
        let cache_key = hash_bytes(data);

        // Get or create texture
        let texture = self.bitmap_cache.entry(cache_key).or_insert_with(|| {
            let rgb_image = PrinterState::bitmap_to_rgb(width_px, height_px, data);
            let size = [rgb_image.width() as usize, rgb_image.height() as usize];
            let pixels: Vec<egui::Color32> = rgb_image
                .pixels()
                .map(|p| egui::Color32::from_rgb(p[0], p[1], p[2]))
                .collect();
            let color_image = ColorImage { size, pixels };
            ui.ctx().load_texture(
                format!("bitmap_{}", cache_key),
                color_image,
                TextureOptions::NEAREST,
            )
        });

        // Scale down to fit receipt viewer — max display width ~400px
        let scale = (400.0 / width_px as f32).min(1.0);
        let display_size = egui::vec2(width_px as f32 * scale, height_px as f32 * scale);
        ui.image((texture.id(), display_size));
    }
}
