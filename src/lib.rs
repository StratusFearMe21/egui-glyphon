//! This crate is for using [`glyphon`] to render advanced shaped text to the screen in an [`egui`] application
//! Please see the example for a primer on how to use this crate
use std::ops::DerefMut;
use std::sync::Arc;

use egui::mutex::{Mutex, RwLock};
use egui::{Pos2, Rect, Vec2};
use egui_wgpu::wgpu;
use egui_wgpu::ScreenDescriptor;
use glyphon::{
    Buffer, Color, ColorMode, FontSystem, PrepareError, Resolution, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer,
};

pub use glyphon;

/// A text buffer with some accosiated data used to construect a [`glyphon::TextArea`]
pub struct BufferWithTextArea<T> {
    pub buffer: Arc<RwLock<Buffer>>,
    pub rect: Rect,
    pub scale: f32,
    pub opacity: f32,
    pub default_color: Color,
    pub associated_data: T,
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

    let (max_width, max_height) = buffer.size();

    Rect::from_min_size(
        Pos2::ZERO,
        Vec2::new(
            if rtl {
                max_width.unwrap_or(width)
            } else {
                width.min(max_width.unwrap_or(width))
            },
            (total_lines as f32 * buffer.metrics().line_height).min(max_height.unwrap_or(f32::MAX)),
        ),
    )
}

impl<T> BufferWithTextArea<T> {
    pub fn new(
        buffer: Arc<RwLock<Buffer>>,
        rect: Rect,
        opacity: f32,
        default_color: Color,
        ctx: &egui::Context,
        associated_data: T,
    ) -> Self {
        let ppi = ctx.pixels_per_point();
        let rect = rect * ppi;
        BufferWithTextArea {
            buffer,
            rect,
            scale: ppi,
            opacity,
            default_color,
            associated_data,
        }
    }
}

/// A type which must be inserted into the [`egui_wgpu::RenderState`] before any text rendering can happen. Do this with [`GlyphonRenderer::insert`]
pub struct GlyphonRenderer {
    font_system: Arc<Mutex<FontSystem>>,
    cache: SwashCache,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    viewport: glyphon::Viewport,
}

impl GlyphonRenderer {
    /// Insert an instance of itself into the [`egui_wgpu::RenderState`]
    pub fn insert(
        wgpu_render_state: &egui_wgpu::RenderState,
        font_system: Arc<Mutex<FontSystem>>,
        glyphon_cache: &glyphon::Cache,
        glyphon_viewport: glyphon::Viewport,
    ) {
        let device = &wgpu_render_state.device;
        let queue = &wgpu_render_state.queue;

        let cache = SwashCache::new();
        let mut atlas = TextAtlas::with_color_mode(
            device,
            queue,
            glyphon_cache,
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
                atlas,
                text_renderer,
                viewport: glyphon_viewport,
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
}

/// A callback which can be put into an [`egui_wgpu::renderer::Callback`].
// And wrapped with an [`egui::PaintCallback`]. Only add one callback per individual
// deffered viewport.
pub struct GlyphonRendererCallback<T> {
    /// These buffers will be rendered to the screen all at the same time on the same layer.
    pub buffers: Vec<BufferWithTextArea<T>>,
}

impl<T: Sync + Send> egui_wgpu::CallbackTrait for GlyphonRendererCallback<T> {
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
                custom_glyphs: &[],
                opacity: b.opacity,
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

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        render_pass.set_viewport(
            0.0,
            0.0,
            info.screen_size_px[0] as f32,
            info.screen_size_px[1] as f32,
            0.0,
            1.0,
        );
        let glyphon_renderer: &GlyphonRenderer = callback_resources.get().unwrap();
        glyphon_renderer
            .text_renderer
            .render(
                &glyphon_renderer.atlas,
                &glyphon_renderer.viewport,
                render_pass,
            )
            .unwrap()
    }
}
