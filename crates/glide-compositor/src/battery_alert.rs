use crate::system_info::PowerProfile;

pub const CRITICAL_PCT: u8 = 5;
const RECOVERY_MARGIN: u8 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    #[default]
    Normal,
    Low,
    Critical,
}

#[derive(Debug, Default)]
pub struct BatteryAlert {
    announced: Level,
}

impl BatteryAlert {
    pub fn update(&mut self, pct: Option<u8>, discharging: bool, low_pct: u8) -> Option<Level> {
        let Some(pct) = pct.filter(|_| discharging) else {
            self.announced = Level::Normal;
            return None;
        };
        let level = if pct <= CRITICAL_PCT.min(low_pct) {
            Level::Critical
        } else if pct <= low_pct {
            Level::Low
        } else {
            Level::Normal
        };
        if level > self.announced {
            self.announced = level;
            return Some(level);
        }
        if pct > low_pct.saturating_add(RECOVERY_MARGIN) {
            self.announced = Level::Normal;
        }
        None
    }
}

#[derive(Debug, Default)]
pub struct Saver {
    restore: Option<PowerProfile>,
}

impl Saver {
    pub fn update(
        &mut self, enabled: bool, pct: Option<u8>, discharging: bool, low_pct: u8, current: Option<PowerProfile>,
    ) -> Option<PowerProfile> {
        match self.restore {
            None => {
                let low = discharging && pct.is_some_and(|pct| pct <= low_pct);
                let previous = current.filter(|profile| *profile != PowerProfile::PowerSaver);
                let previous = previous.filter(|_| enabled && low)?;
                self.restore = Some(previous);
                Some(PowerProfile::PowerSaver)
            }
            Some(previous) if !discharging => {
                self.restore = None;
                (current == Some(PowerProfile::PowerSaver)).then_some(previous)
            }
            Some(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_saver_steps_in_at_the_warning_and_hands_back_on_the_charger() {
        let mut saver = Saver::default();
        let balanced = Some(PowerProfile::Balanced);
        assert_eq!(saver.update(true, Some(40), true, 20, balanced), None);
        assert_eq!(saver.update(true, Some(20), true, 20, balanced), Some(PowerProfile::PowerSaver));
        let saving = Some(PowerProfile::PowerSaver);
        assert_eq!(saver.update(true, Some(18), true, 20, saving), None, "switched once");
        assert_eq!(saver.update(true, Some(18), false, 20, saving), Some(PowerProfile::Balanced));
        assert_eq!(saver.update(true, Some(19), false, 20, balanced), None);
    }

    #[test]
    fn the_saver_stays_away_when_off_already_saving_or_overruled() {
        let mut saver = Saver::default();
        assert_eq!(saver.update(false, Some(10), true, 20, Some(PowerProfile::Performance)), None);
        assert_eq!(saver.update(true, Some(10), true, 20, Some(PowerProfile::PowerSaver)), None);
        assert_eq!(saver.update(true, Some(10), true, 20, None), None, "no power-profiles-daemon");
        assert_eq!(
            saver.update(true, Some(10), true, 20, Some(PowerProfile::Performance)),
            Some(PowerProfile::PowerSaver)
        );
        assert_eq!(
            saver.update(true, Some(10), false, 20, Some(PowerProfile::Balanced)),
            None,
            "the user already picked another profile, so it is left alone"
        );
    }

    fn run(alert: &mut BatteryAlert, samples: &[(u8, bool)]) -> Vec<Level> {
        samples.iter().filter_map(|&(pct, discharging)| alert.update(Some(pct), discharging, 20)).collect()
    }

    #[test]
    fn each_threshold_is_announced_once_on_the_way_down() {
        let mut alert = BatteryAlert::default();
        let drain: Vec<(u8, bool)> = (1..=30).rev().map(|pct| (pct, true)).collect();
        assert_eq!(run(&mut alert, &drain), [Level::Low, Level::Critical]);
    }

    #[test]
    fn a_reading_that_wobbles_around_the_threshold_does_not_repeat_the_warning() {
        let mut alert = BatteryAlert::default();
        let wobble = [(21, true), (20, true), (21, true), (20, true), (22, true), (19, true)];
        assert_eq!(run(&mut alert, &wobble), [Level::Low]);
    }

    #[test]
    fn charging_or_a_real_recovery_arms_the_warning_again() {
        let mut alert = BatteryAlert::default();
        assert_eq!(run(&mut alert, &[(15, true), (15, false), (15, true)]), [Level::Low, Level::Low]);
        assert_eq!(run(&mut alert, &[(24, true), (18, true)]), [Level::Low], "24 % is past the recovery margin");
    }

    #[test]
    fn nothing_is_said_while_charging_or_without_a_battery() {
        let mut alert = BatteryAlert::default();
        assert_eq!(run(&mut alert, &[(3, false), (2, false)]), []);
        assert_eq!(alert.update(None, true, 20), None);
    }

    #[test]
    fn a_low_threshold_under_five_percent_goes_straight_to_critical() {
        let mut alert = BatteryAlert::default();
        assert_eq!(alert.update(Some(4), true, 3), None);
        assert_eq!(alert.update(Some(3), true, 3), Some(Level::Critical));
    }
}
