pub const R1: f32 = 4.0;
pub const R2: f32 = 8.0;
pub const R3: f32 = 12.0;
pub const R4: f32 = 18.0;
pub const R_ROUND: f32 = 999.0;

pub const BUTTON_RADIUS: f32 = 6.0;

#[cfg(test)]
#[allow(clippy::assertions_on_constants)]
mod tests {
    use super::*;

    #[test]
    fn radii_strictly_increase_from_micro_to_shell() {
        assert!(R1 < R2);
        assert!(R2 < R3);
        assert!(R3 < R4);
        assert!(R4 < R_ROUND);
    }

    #[test]
    fn button_radius_sits_between_micro_and_control() {
        assert!(BUTTON_RADIUS > R1);
        assert!(BUTTON_RADIUS < R2);
    }
}
