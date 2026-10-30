use std::sync::LazyLock;

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Color32F, ContextId, DebugFlags, Frame, ImportAll, ImportMem, Renderer, RendererSuper, Texture,
            TextureFilter, sync::SyncPoint,
        },
    },
    reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    utils::{Buffer, Physical, Rectangle, Size, Transform},
    wayland::compositor::SurfaceData,
};

static CONTEXT: LazyLock<ContextId<ProbeTexture>> = LazyLock::new(ContextId::new);

#[derive(Debug)]
pub struct ProbeError;

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("probe renderer error")
    }
}

impl std::error::Error for ProbeError {}

#[derive(Debug, Clone)]
pub struct ProbeTexture {
    size: Size<i32, Buffer>,
}

impl Texture for ProbeTexture {
    fn width(&self) -> u32 {
        self.size.w as u32
    }

    fn height(&self) -> u32 {
        self.size.h as u32
    }

    fn format(&self) -> Option<Fourcc> {
        None
    }
}

#[derive(Debug)]
pub struct ProbeTarget;

impl Texture for ProbeTarget {
    fn width(&self) -> u32 {
        0
    }

    fn height(&self) -> u32 {
        0
    }

    fn format(&self) -> Option<Fourcc> {
        None
    }
}

#[derive(Debug)]
pub struct ProbeFrame;

impl Frame for ProbeFrame {
    type Error = ProbeError;
    type TextureId = ProbeTexture;

    fn context_id(&self) -> ContextId<ProbeTexture> {
        CONTEXT.clone()
    }

    fn clear(&mut self, _color: Color32F, _at: &[Rectangle<i32, Physical>]) -> Result<(), ProbeError> {
        Ok(())
    }

    fn draw_solid(
        &mut self, _dst: Rectangle<i32, Physical>, _damage: &[Rectangle<i32, Physical>], _color: Color32F,
    ) -> Result<(), ProbeError> {
        Ok(())
    }

    fn render_texture_from_to(
        &mut self, _texture: &ProbeTexture, _src: Rectangle<f64, Buffer>, _dst: Rectangle<i32, Physical>,
        _damage: &[Rectangle<i32, Physical>], _opaque_regions: &[Rectangle<i32, Physical>], _src_transform: Transform,
        _alpha: f32,
    ) -> Result<(), ProbeError> {
        Ok(())
    }

    fn transformation(&self) -> Transform {
        Transform::Normal
    }

    fn output_size(&self) -> Size<i32, Physical> {
        Size::default()
    }

    fn wait(&mut self, _sync: &SyncPoint) -> Result<(), ProbeError> {
        Ok(())
    }

    fn finish(self) -> Result<SyncPoint, ProbeError> {
        Ok(SyncPoint::default())
    }
}

#[derive(Debug)]
pub struct Probe;

impl RendererSuper for Probe {
    type Error = ProbeError;
    type TextureId = ProbeTexture;
    type Framebuffer<'buffer> = ProbeTarget;
    type Frame<'frame, 'buffer>
        = ProbeFrame
    where
        'buffer: 'frame,
        Self: 'frame;
}

impl Renderer for Probe {
    fn context_id(&self) -> ContextId<ProbeTexture> {
        CONTEXT.clone()
    }

    fn downscale_filter(&mut self, _filter: TextureFilter) -> Result<(), ProbeError> {
        Ok(())
    }

    fn upscale_filter(&mut self, _filter: TextureFilter) -> Result<(), ProbeError> {
        Ok(())
    }

    fn set_debug_flags(&mut self, _flags: DebugFlags) {}

    fn debug_flags(&self) -> DebugFlags {
        DebugFlags::empty()
    }

    fn render<'frame, 'buffer>(
        &'frame mut self, _framebuffer: &'frame mut ProbeTarget, _output_size: Size<i32, Physical>,
        _dst_transform: Transform,
    ) -> Result<ProbeFrame, ProbeError>
    where
        'buffer: 'frame,
    {
        Ok(ProbeFrame)
    }

    fn wait(&mut self, _sync: &SyncPoint) -> Result<(), ProbeError> {
        Ok(())
    }
}

impl ImportMem for Probe {
    fn import_memory(
        &mut self, _data: &[u8], _format: Fourcc, size: Size<i32, Buffer>, _flipped: bool,
    ) -> Result<ProbeTexture, ProbeError> {
        Ok(ProbeTexture { size })
    }

    fn update_memory(
        &mut self, _texture: &ProbeTexture, _data: &[u8], _region: Rectangle<i32, Buffer>,
    ) -> Result<(), ProbeError> {
        Ok(())
    }

    fn mem_formats(&self) -> Box<dyn Iterator<Item = Fourcc>> {
        Box::new([Fourcc::Argb8888].into_iter())
    }
}

impl ImportAll for Probe {
    fn import_buffer(
        &mut self, _buffer: &WlBuffer, _surface: Option<&SurfaceData>, _damage: &[Rectangle<i32, Buffer>],
    ) -> Option<Result<ProbeTexture, ProbeError>> {
        None
    }
}
