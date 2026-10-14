pub const R1: f32 = 4.0;
pub const R2: f32 = 8.0;
pub const R3: f32 = 12.0;
pub const R4: f32 = 18.0;
pub const R_ROUND: f32 = 999.0;

pub const SCALE: [f32; 5] = [R1, R2, R3, R4, R_ROUND];

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = assert!(R1 < R2 && R2 < R3 && R3 < R4 && R4 < R_ROUND);

    #[test]
    fn theme_defaults_use_only_radii_from_the_scale() {
        let dock = crate::DockTheme::default();
        let deco = crate::DecorationTheme::default();
        let overview = crate::OverviewTheme::default();
        for radius in [dock.radius, dock.item_radius, deco.corner_radius, overview.corner_radius] {
            assert!(SCALE.contains(&radius), "{radius}");
        }
    }
}
