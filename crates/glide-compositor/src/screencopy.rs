use std::time::Duration;

use smithay::{
    output::Output,
    reexports::{
        wayland_protocols_wlr::screencopy::v1::server::{
            zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
            zwlr_screencopy_manager_v1::{self, ZwlrScreencopyManagerV1},
        },
        wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
            protocol::{wl_buffer::WlBuffer, wl_output::WlOutput, wl_shm},
        },
    },
    utils::{Physical, Rectangle},
};

use crate::state::GlideState;

const VERSION: u32 = 3;

#[derive(Debug)]
pub struct PendingCapture {
    pub frame: ZwlrScreencopyFrameV1,
    pub buffer: WlBuffer,
    pub output: Output,
    pub region: Rectangle<i32, Physical>,
    pub with_damage: bool,
}

#[derive(Debug)]
pub struct FrameState {
    output: Option<Output>,
    region: Rectangle<i32, Physical>,
    used: std::sync::atomic::AtomicBool,
}

impl FrameState {
    fn orphan() -> Self {
        Self {
            output: None,
            region: Rectangle::default(),
            used: std::sync::atomic::AtomicBool::new(true),
        }
    }
}

#[derive(Debug)]
pub struct ScreencopyState {
    pending: Vec<PendingCapture>,
}

impl ScreencopyState {
    #[must_use]
    pub fn new<D>(display: &DisplayHandle) -> Self
    where
        D: GlobalDispatch<ZwlrScreencopyManagerV1, ()> + 'static,
    {
        display.create_global::<D, ZwlrScreencopyManagerV1, _>(VERSION, ());
        Self {
            pending: Vec::new(),
        }
    }

    pub fn take_for(&mut self, output: &Output) -> Vec<PendingCapture> {
        let mut taken = Vec::new();
        let mut index = 0;
        while index < self.pending.len() {
            if &self.pending[index].output == output {
                taken.push(self.pending.remove(index));
            } else {
                index += 1;
            }
        }
        taken
    }

    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

impl GlobalDispatch<ZwlrScreencopyManagerV1, ()> for GlideState {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrScreencopyManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for GlideState {
    fn request(
        state: &mut Self,
        _client: &Client,
        _manager: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let (frame, output, region) = match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput {
                frame,
                overlay_cursor: _,
                output,
            } => {
                let Some(output) = state.output_for_resource(&output) else {
                    let frame = data_init.init(frame, FrameState::orphan());
                    frame.failed();
                    return;
                };
                let region = output_rect(&output);
                (frame, output, region)
            }
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                overlay_cursor: _,
                output,
                x,
                y,
                width,
                height,
            } => {
                let Some(output) = state.output_for_resource(&output) else {
                    let frame = data_init.init(frame, FrameState::orphan());
                    frame.failed();
                    return;
                };
                let full = output_rect(&output);
                let requested = Rectangle::new((x, y).into(), (width, height).into());
                let Some(region) = full.intersection(requested) else {
                    let frame = data_init.init(frame, FrameState::orphan());
                    frame.failed();
                    return;
                };
                (frame, output, region)
            }
            _ => return,
        };

        if region.size.w <= 0 || region.size.h <= 0 {
            let frame = data_init.init(frame, FrameState::orphan());
            frame.failed();
            return;
        }

        let frame = data_init.init(
            frame,
            FrameState {
                output: Some(output),
                region,
                used: std::sync::atomic::AtomicBool::new(false),
            },
        );

        #[allow(clippy::cast_sign_loss)]
        let stride = (region.size.w * 4) as u32;
        #[allow(clippy::cast_sign_loss)]
        frame.buffer(
            wl_shm::Format::Argb8888,
            region.size.w as u32,
            region.size.h as u32,
            stride,
        );
        if frame.version() >= 3 {
            frame.buffer_done();
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, FrameState> for GlideState {
    fn request(
        state: &mut Self,
        _client: &Client,
        frame: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        data: &FrameState,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let (buffer, with_damage) = match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => (buffer, false),
            zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => (buffer, true),
            zwlr_screencopy_frame_v1::Request::Destroy => {
                state
                    .screencopy
                    .pending
                    .retain(|pending| &pending.frame != frame);
                return;
            }
            _ => return,
        };

        if data.used.swap(true, std::sync::atomic::Ordering::Relaxed) {
            frame.post_error(
                zwlr_screencopy_frame_v1::Error::AlreadyUsed,
                "frame was already copied",
            );
            return;
        }

        let Some(output) = data.output.clone() else {
            frame.failed();
            return;
        };

        if !buffer_is_suitable(&buffer, data.region) {
            frame.post_error(
                zwlr_screencopy_frame_v1::Error::InvalidBuffer,
                "buffer does not match the advertised format or size",
            );
            return;
        }

        state.screencopy.pending.push(PendingCapture {
            frame: frame.clone(),
            buffer,
            output,
            region: data.region,
            with_damage,
        });
        state.queue_redraw();
    }

    fn destroyed(
        state: &mut Self,
        _client: smithay::reexports::wayland_server::backend::ClientId,
        frame: &ZwlrScreencopyFrameV1,
        _data: &FrameState,
    ) {
        state
            .screencopy
            .pending
            .retain(|pending| &pending.frame != frame);
    }
}

fn output_rect(output: &Output) -> Rectangle<i32, Physical> {
    let size = output
        .current_mode()
        .map_or_else(|| (0, 0).into(), |mode| mode.size);
    Rectangle::new((0, 0).into(), size)
}

fn buffer_is_suitable(buffer: &WlBuffer, region: Rectangle<i32, Physical>) -> bool {
    smithay::wayland::shm::with_buffer_contents(buffer, |_ptr, len, data| {
        let expected_stride = region.size.w * 4;
        let needed = usize::try_from(data.stride.max(0) * data.height.max(0)).unwrap_or(usize::MAX);
        data.width == region.size.w
            && data.height == region.size.h
            && data.stride >= expected_stride
            && matches!(
                data.format,
                wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888
            )
            && len >= needed
    })
    .unwrap_or(false)
}

impl GlideState {
    fn output_for_resource(&self, resource: &WlOutput) -> Option<Output> {
        self.space
            .outputs()
            .find(|output| output.owns(resource))
            .cloned()
    }

    pub fn serve_screencopy(
        &mut self,
        output: &Output,
        pixels: &[u8],
        size: smithay::utils::Size<i32, Physical>,
        presented: Duration,
    ) {
        let captures = self.screencopy.take_for(output);
        if captures.is_empty() {
            return;
        }

        let transform = output.current_transform();
        for capture in captures {
            let copied = copy_region(&capture, pixels, size);
            if !copied {
                capture.frame.failed();
                continue;
            }

            capture
                .frame
                .flags(zwlr_screencopy_frame_v1::Flags::empty());
            if capture.with_damage {
                #[allow(clippy::cast_sign_loss)]
                capture.frame.damage(
                    0,
                    0,
                    capture.region.size.w as u32,
                    capture.region.size.h as u32,
                );
            }

            #[allow(clippy::cast_possible_truncation)]
            let tv_sec = presented.as_secs();
            capture.frame.ready(
                (tv_sec >> 32) as u32,
                (tv_sec & 0xFFFF_FFFF) as u32,
                presented.subsec_nanos(),
            );
            let _ = transform;
        }
    }
}

fn copy_region(
    capture: &PendingCapture,
    pixels: &[u8],
    frame_size: smithay::utils::Size<i32, Physical>,
) -> bool {
    let region = capture.region;
    if region.loc.x < 0
        || region.loc.y < 0
        || region.loc.x + region.size.w > frame_size.w
        || region.loc.y + region.size.h > frame_size.h
    {
        return false;
    }

    let to_usize = |value: i32| usize::try_from(value).unwrap_or(0);
    let source_stride = to_usize(frame_size.w) * 4;

    smithay::wayland::shm::with_buffer_contents_mut(&capture.buffer, |ptr, len, data| {
        let dest_stride = to_usize(data.stride);
        let row_bytes = to_usize(region.size.w) * 4;

        for row in 0..region.size.h {
            let source_offset =
                to_usize(region.loc.y + row) * source_stride + to_usize(region.loc.x) * 4;
            let dest_offset = to_usize(row) * dest_stride;
            if source_offset + row_bytes > pixels.len() || dest_offset + row_bytes > len {
                return false;
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pixels.as_ptr().add(source_offset),
                    ptr.add(dest_offset),
                    row_bytes,
                );
            }
        }
        true
    })
    .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rectangle<i32, Physical> {
        Rectangle::new((x, y).into(), (w, h).into())
    }

    #[test]
    fn the_advertised_stride_is_four_bytes_a_pixel() {
        let region = rect(0, 0, 1920, 1080);
        assert_eq!(region.size.w * 4, 7680);
    }

    #[test]
    fn a_region_outside_the_frame_is_rejected() {
        let frame_size: smithay::utils::Size<i32, Physical> = (100, 100).into();

        for region in [
            rect(-1, 0, 10, 10),
            rect(0, -1, 10, 10),
            rect(95, 0, 10, 10),
            rect(0, 95, 10, 10),
        ] {
            let out_of_bounds = region.loc.x < 0
                || region.loc.y < 0
                || region.loc.x + region.size.w > frame_size.w
                || region.loc.y + region.size.h > frame_size.h;
            assert!(out_of_bounds, "{region:?} should be rejected");
        }
    }

    #[test]
    fn a_region_inside_the_frame_is_accepted() {
        let frame_size: smithay::utils::Size<i32, Physical> = (100, 100).into();
        let region = rect(10, 10, 80, 80);
        let out_of_bounds = region.loc.x < 0
            || region.loc.y < 0
            || region.loc.x + region.size.w > frame_size.w
            || region.loc.y + region.size.h > frame_size.h;
        assert!(!out_of_bounds);
    }

    #[test]
    fn a_full_output_region_starts_at_the_origin() {
        let region = rect(0, 0, 2560, 1440);
        assert_eq!(region.loc.x, 0);
        assert_eq!(region.loc.y, 0);
    }
}
