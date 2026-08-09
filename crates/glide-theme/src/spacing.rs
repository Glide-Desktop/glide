pub const SP1: i32 = 4;
pub const SP2: i32 = 8;
pub const SP3: i32 = 12;
pub const SP4: i32 = 16;
pub const SP5: i32 = 24;
pub const SP6: i32 = 32;
pub const SP7: i32 = 48;
pub const SP8: i32 = 64;

pub const STEPS: [i32; 8] = [SP1, SP2, SP3, SP4, SP5, SP6, SP7, SP8];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_is_a_multiple_of_the_base_unit() {
        for step in STEPS {
            assert_eq!(step % 4, 0, "{step} is not a multiple of the 4px base step");
        }
    }

    #[test]
    fn steps_strictly_increase() {
        for pair in STEPS.windows(2) {
            assert!(pair[0] < pair[1]);
        }
    }
}
