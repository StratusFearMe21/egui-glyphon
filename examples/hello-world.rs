#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use std::sync::Arc;

use eframe::{
    egui::{self, Slider},
    egui_wgpu,
    epaint::{
        mutex::{Mutex, RwLock},
        Rect, Vec2,
    },
    CreationContext,
};
use egui_glyphon::{
    glyphon::{Attrs, Family, FontSystem, Metrics, Shaping},
    BufferWithTextArea, GlyphonRenderer, GlyphonRendererCallback,
};
use glyphon::Buffer;

fn main() -> Result<(), eframe::Error> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    eframe::run_native(
        "My egui App",
        options,
        Box::new(|cc| Box::new(MyApp::new(cc))),
    )
}

struct MyApp {
    font_system: Arc<Mutex<FontSystem>>,
    size: f32,
    buffer: Arc<RwLock<Buffer>>,
}

impl Default for MyApp {
    fn default() -> Self {
        let mut font_system = FontSystem::new();
        let mut buffer =
            egui_glyphon::glyphon::Buffer::new(&mut font_system, Metrics::new(30.0, 42.0));

        buffer.set_size(&mut font_system, 16.0, 9.0);
        buffer.set_text(&mut font_system, "<== Hello world! ==> 👋\nThis is rendered with 🦅 glyphon 🦁\nThe text below should be partially clipped.\na b c d e f g h i j k l m n o p q r s t u v w x y z fi ffi 🐕‍🦺 fi ffi
        fi تما 🐕‍🦺 ffi تما
        ffi fi 🐕‍🦺 ffi fi
        تما تما 🐕‍🦺 تما
        تما ffi 🐕‍🦺 تما fi تما
        تما تما 🐕‍🦺 تما", Attrs::new().family(Family::SansSerif), Shaping::Advanced);
        buffer.shape_until_scroll(&mut font_system);
        Self {
            font_system: Arc::new(Mutex::new(font_system)),
            buffer: Arc::new(RwLock::new(buffer)),
            size: 35.0,
        }
    }
}

impl MyApp {
    fn new(cc: &CreationContext<'_>) -> Self {
        let app = Self::default();

        if let Some(ref wgpu) = cc.wgpu_render_state {
            GlyphonRenderer::insert(wgpu, Arc::clone(&app.font_system));
        }

        app
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let size = Vec2::new(16.0 * self.size, 9.0 * self.size);

        {
            let mut font_system = self.font_system.lock();
            let mut buffer = self.buffer.write();
            buffer.set_metrics(&mut font_system, Metrics::new(self.size, self.size));
            buffer.set_size(&mut font_system, size.x, size.y);
            buffer.shape_until_scroll(&mut font_system);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add(Slider::new(&mut self.size, 0.1..=67.5));
            let rect = Rect::from_min_size(ui.cursor().min, size);
            let buffers: Vec<BufferWithTextArea> = vec![BufferWithTextArea::new(
                self.buffer.clone(),
                rect,
                1.0,
                egui_glyphon::glyphon::Color::rgb(255, 255, 255),
                ui.ctx(),
            )];
            ui.painter().add(egui_wgpu::Callback::new_paint_callback(
                ui.max_rect(),
                GlyphonRendererCallback { buffers },
            ));
        });
    }
}
