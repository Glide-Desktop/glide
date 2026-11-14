use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use crate::biometrics::{DetectedFace, FACE_MESH_CONTOUR_EDGES, FACE_MESH_TRIANGULATION_EDGES, RgbaImage};

fn paint(rgba: [u8; 4]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3]));
    paint.anti_alias = true;
    paint
}

fn stroke(w: f32) -> Stroke {
    Stroke { width: w, ..Default::default() }
}

#[must_use]
pub fn annotate(src: &RgbaImage, face: &DetectedFace) -> RgbaImage {
    let Some(mut pm) = Pixmap::new(src.width as u32, src.height as u32) else { return src.clone() };
    pm.data_mut().copy_from_slice(&src.pix);
    let t = Transform::identity();

    if let Some(rect) = Rect::from_xywh(face.x, face.y, face.width, face.height) {
        let path = PathBuilder::from_rect(rect);
        pm.stroke_path(&path, &paint([56, 189, 248, 255]), &stroke(2.5), t, None);
    }

    let edges = |pm: &mut Pixmap, list: &[[usize; 2]], p: &Paint, w: f32| {
        let mut pb = PathBuilder::new();
        for e in list {
            let (a, b) = (face.dense[e[0]], face.dense[e[1]]);
            pb.move_to(a.x, a.y);
            pb.line_to(b.x, b.y);
        }
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, p, &stroke(w), t, None);
        }
    };
    let dots = |pm: &mut Pixmap, pts: &mut dyn Iterator<Item = (f32, f32)>, r: f32| {
        let mut pb = PathBuilder::new();
        for (x, y) in pts {
            pb.push_circle(x, y, r);
        }
        if let Some(path) = pb.finish() {
            pm.fill_path(&path, &paint([52, 211, 153, 255]), FillRule::Winding, t, None);
        }
    };

    if face.has_dense() {
        edges(&mut pm, FACE_MESH_TRIANGULATION_EDGES, &paint([56, 189, 248, 80]), 1.0);
        edges(&mut pm, FACE_MESH_CONTOUR_EDGES, &paint([56, 189, 248, 180]), 1.5);
        dots(&mut pm, &mut face.dense.iter().map(|p| (p.x, p.y)), 3.0);
    } else {
        dots(&mut pm, &mut face.landmarks.as_array().into_iter().map(|p| (p.x, p.y)), 4.0);
    }

    RgbaImage::from_raw(src.width, src.height, pm.take())
}
