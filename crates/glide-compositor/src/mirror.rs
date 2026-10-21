use smithay::output::Output;
use smithay::utils::{Physical, Point};

use crate::GlideState;
use crate::config::Arrangement;

#[must_use]
pub fn primary<'a>(candidates: &[&'a str]) -> Option<&'a str> {
    let mut sorted = candidates.to_vec();
    sorted.sort_unstable();
    sorted.iter().find(|name| crate::lid::is_internal(name)).or_else(|| sorted.first()).copied()
}

#[must_use]
pub fn fit(source: (i32, i32), target: (i32, i32)) -> (f64, Point<i32, Physical>) {
    if source.0 <= 0 || source.1 <= 0 || target.0 <= 0 || target.1 <= 0 {
        return (1.0, Point::from((0, 0)));
    }
    let factor = (f64::from(target.0) / f64::from(source.0)).min(f64::from(target.1) / f64::from(source.1));
    let drawn = |side: i32| (f64::from(side) * factor).round() as i32;
    let offset = ((target.0 - drawn(source.0)) / 2, (target.1 - drawn(source.1)) / 2);
    (factor, Point::from(offset))
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub map: Vec<String>,
    pub unmap: Vec<String>,
}

#[must_use]
pub fn plan(arrangement: Arrangement, mapped: &[&str], mirrors: &[&str]) -> Plan {
    match arrangement {
        Arrangement::Extend => Plan { map: mirrors.iter().map(ToString::to_string).collect(), unmap: Vec::new() },
        Arrangement::Mirror => {
            let candidates: Vec<&str> = mapped.iter().chain(mirrors).copied().collect();
            let Some(primary) = primary(&candidates) else {
                return Plan::default();
            };
            Plan {
                map: mirrors.iter().filter(|name| **name == primary).map(ToString::to_string).collect(),
                unmap: mapped.iter().filter(|name| **name != primary).map(ToString::to_string).collect(),
            }
        }
    }
}

impl GlideState {
    pub fn apply_arrangement(&mut self) {
        let mapped: Vec<Output> = self.space.outputs().cloned().collect();
        let mapped_names: Vec<String> = mapped.iter().map(Output::name).collect();
        let mirror_names: Vec<String> = self.session.mirrors.iter().map(Output::name).collect();
        let mapped_refs: Vec<&str> = mapped_names.iter().map(String::as_str).collect();
        let mirror_refs: Vec<&str> = mirror_names.iter().map(String::as_str).collect();
        let plan = plan(self.session.arrangement, &mapped_refs, &mirror_refs);
        for name in &plan.unmap {
            if let Some(output) = mapped.iter().find(|output| output.name() == *name) {
                tracing::info!(output = %name, "mirroring the primary display");
                self.space.unmap_output(output);
                self.session.mirrors.push(output.clone());
            }
        }
        for name in &plan.map {
            if let Some(index) = self.session.mirrors.iter().position(|output| output.name() == *name) {
                let output = self.session.mirrors.remove(index);
                self.space.map_output(&output, output.current_location());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_internal_panel_leads_the_mirror_and_otherwise_the_first_connector_by_name() {
        assert_eq!(primary(&["HDMI-A-1", "eDP-1"]), Some("eDP-1"));
        assert_eq!(primary(&["HDMI-A-1", "DP-2"]), Some("DP-2"));
        assert_eq!(primary(&[]), None);
    }

    #[test]
    fn mirroring_unmaps_all_but_the_primary_and_extending_maps_the_copies_back() {
        let mirror = plan(Arrangement::Mirror, &["eDP-1", "HDMI-A-1"], &[]);
        assert_eq!(mirror, Plan { map: Vec::new(), unmap: vec!["HDMI-A-1".into()] });
        let lid_closed = plan(Arrangement::Mirror, &[], &["HDMI-A-1"]);
        assert_eq!(
            lid_closed,
            Plan { map: vec!["HDMI-A-1".into()], unmap: Vec::new() },
            "the external display takes over"
        );
        let lid_open = plan(Arrangement::Mirror, &["eDP-1", "HDMI-A-1"], &[]);
        assert_eq!(lid_open.unmap, ["HDMI-A-1"]);
        let extend = plan(Arrangement::Extend, &["eDP-1"], &["HDMI-A-1"]);
        assert_eq!(extend, Plan { map: vec!["HDMI-A-1".into()], unmap: Vec::new() });
        assert_eq!(plan(Arrangement::Mirror, &["eDP-1"], &[]), Plan::default(), "one display is left alone");
    }

    #[test]
    fn a_copy_keeps_the_aspect_ratio_with_black_bars() {
        assert_eq!(fit((1920, 1080), (3840, 2160)), (2.0, Point::from((0, 0))));
        let (factor, offset) = fit((1920, 1080), (1920, 1200));
        assert!((factor - 1.0).abs() < 1e-9);
        assert_eq!(offset, Point::from((0, 60)), "letterboxed top and bottom");
        let (factor, offset) = fit((2560, 1600), (1920, 1080));
        assert!((factor - 0.675).abs() < 1e-9);
        assert_eq!(offset, Point::from((96, 0)), "pillarboxed left and right");
        assert_eq!(fit((0, 0), (1920, 1080)).0, 1.0);
    }
}
