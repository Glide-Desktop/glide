use glide_video::gstreamer as gst;

#[must_use]
pub fn rgba(sample: &gst::Sample) -> Option<image::RgbaImage> {
    let frame = glide_video::frames::sample_rgba(sample)?;
    image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
}
