//! This crate is for using [`glyphon`] to render advanced shaped text to the screen in an [`egui`] application
//! Please see the example for a primer on how to use this crate
use std::ops::DerefMut;
use std::sync::Arc;

use egui::mutex::{Mutex, RwLock};
use egui::{Pos2, Rect, Vec2};
use egui_wgpu::wgpu;
use egui_wgpu::ScreenDescriptor;
use glyphon::{
    Buffer, Color, ColorMode, FontSystem, PrepareError, RenderError, Resolution, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport
};

pub use glyphon;

/// A text buffer with some accosiated data used to construect a [`glyphon::TextArea`]
pub struct BufferWithTextArea {
    pub buffer: Arc<RwLock<Buffer>>,
    pub rect: Rect,
    pub scale: f32,
    pub opacity: f32,
    pub default_color: Color,
}

/// Use this function to find out the dimensions of a buffer, translate the resulting rect and use it in [`BufferWithTextArea::new`]
pub fn measure_buffer(buffer: &Buffer) -> Rect {
    let mut rtl = false;
    let (width, total_lines) =
        buffer
            .layout_runs()
            .fold((0.0, 0usize), |(width, total_lines), run| {
                if run.rtl {
                    rtl = true;
                }
                (run.line_w.max(width), total_lines + 1)
            });

    let height = total_lines as f32 * buffer.metrics().line_height;

    let (max_width, max_height) = buffer.size();
    let max_width = max_width.unwrap_or(width);
    let max_height = max_height.unwrap_or(height);

    let width = if rtl { max_width } else { width.min(max_width) };
    let height = height.min(max_height);

    Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))
}

impl BufferWithTextArea {
    pub fn new(
        buffer: Arc<RwLock<Buffer>>,
        rect: Rect,
        opacity: f32,
        default_color: Color,
        ctx: &egui::Context,
    ) -> Self {
        let ppi = ctx.pixels_per_point();
        let rect = rect * ppi;
        BufferWithTextArea {
            buffer,
            rect,
            scale: ppi,
            opacity,
            default_color,
        }
    }
}

/// A type which must be inserted into the [`egui_wgpu::RenderState`] before any text rendering can happen. Do this with [`GlyphonRenderer::insert`]
pub struct GlyphonRenderer {
    font_system: Arc<Mutex<FontSystem>>,
    cache: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    text_renderer: TextRenderer,
}

impl GlyphonRenderer {
    /// Insert an instance of itself into the [`egui_wgpu::RenderState`]
    pub fn insert(wgpu_render_state: &egui_wgpu::RenderState, font_system: Arc<Mutex<FontSystem>>) {
        let device = &wgpu_render_state.device;
        let queue = &wgpu_render_state.queue;

        let cache = SwashCache::new();
        let gcache = glyphon::Cache::new(device);
        let viewport = Viewport::new(device, &gcache);
        let mut atlas = TextAtlas::with_color_mode(
            device,
            queue,
            &gcache,
            wgpu_render_state.target_format,
            ColorMode::Web,
        );
        let text_renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        wgpu_render_state
            .renderer
            .write()
            .callback_resources
            .insert(Self {
                font_system: Arc::clone(&font_system),
                cache,
                viewport,
                atlas,
                text_renderer,
            });
    }

    fn prepare<'a>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_resolution: Resolution,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
    ) -> Result<(), PrepareError> {
        self.viewport.update(queue, screen_resolution);
        self.text_renderer.prepare(
            device,
            queue,
            self.font_system.lock().deref_mut(),
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.cache,
        )
    }

    fn render(&self, pass: &mut wgpu::RenderPass<'static>) -> Result<(), RenderError> {
        self.text_renderer.render(&self.atlas, &self.viewport, pass)
    }
}

/// A callback which can be put into an [`egui_wgpu::renderer::Callback`].
// And wrapped with an [`egui::PaintCallback`]. Only add one callback per individual
// deffered viewport.
pub struct GlyphonRendererCallback {
    /// These buffers will be rendered to the screen all at the same time on the same layer.
    pub buffers: Vec<BufferWithTextArea>,
}

impl egui_wgpu::CallbackTrait for GlyphonRendererCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let glyphon_renderer: &mut GlyphonRenderer = resources.get_mut().unwrap();
        glyphon_renderer.atlas.trim();
        let bufrefs: Vec<_> = self.buffers.iter().map(|b| b.buffer.read()).collect();
        let text_areas: Vec<_> = self
            .buffers
            .iter()
            .enumerate()
            .map(|(i, b)| TextArea {
                custom_glyphs: &[],
                buffer: bufrefs.get(i).unwrap(),
                left: b.rect.left(),
                top: b.rect.top(),
                scale: b.scale,
                bounds: TextBounds {
                    left: b.rect.left() as i32,
                    top: b.rect.top() as i32,
                    right: b.rect.right() as i32,
                    bottom: b.rect.bottom() as i32,
                },
                default_color: b.default_color,
            })
            .collect();
        glyphon_renderer
            .prepare(
                device,
                queue,
                Resolution {
                    width: screen_descriptor.size_in_pixels[0],
                    height: screen_descriptor.size_in_pixels[1],
                },
                text_areas,
            )
            .unwrap();
        Vec::new()
    }

    fn paint<'a>(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        render_pass.set_viewport(
            0.0,
            0.0,
            info.screen_size_px[0] as f32,
            info.screen_size_px[1] as f32,
            0.0,
            1.0,
        );
        let glyphon_renderer: &GlyphonRenderer = resources.get().unwrap();
        glyphon_renderer.render(render_pass).unwrap();
    }
}
