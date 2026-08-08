use smithay::{
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::Serial,
    wayland::xdg_activation::{
        XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
    },
};

use crate::GlideState;

const MAX_TOKEN_AGE: std::time::Duration = std::time::Duration::from_secs(10);

fn token_serial_is_fresh(
    seat_owns_requesting_seat: bool,
    token_serial: Option<Serial>,
    keyboard_last_enter: Option<Serial>,
) -> bool {
    seat_owns_requesting_seat
        && token_serial
            .zip(keyboard_last_enter)
            .is_some_and(|(token, enter)| token.is_no_older_than(&enter))
}

const fn token_is_fresh_enough(age: std::time::Duration) -> bool {
    age.as_secs() <= MAX_TOKEN_AGE.as_secs()
}

impl XdgActivationHandler for GlideState {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.protocols.xdg_activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        let Some((serial, seat)) = data.serial else {
            return false;
        };
        let seat_owns_requesting_seat = self.seat.owns(&seat);
        let last_enter = self
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.last_enter());
        token_serial_is_fresh(seat_owns_requesting_seat, Some(serial), last_enter)
    }

    fn request_activation(
        &mut self,
        _token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if !token_is_fresh_enough(token_data.timestamp.elapsed()) {
            return;
        }
        let Some(id) = self.window_id_for_surface(&surface) else {
            return;
        };
        if self.layout.focus_window(id) {
            self.reconcile_space();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serial(n: u32) -> Serial {
        Serial::from(n)
    }

    #[test]
    fn a_token_with_no_serial_at_all_is_never_fresh() {
        assert!(!token_serial_is_fresh(true, None, Some(serial(5))));
    }

    #[test]
    fn a_token_is_rejected_if_the_seat_never_had_keyboard_focus() {
        assert!(!token_serial_is_fresh(true, Some(serial(5)), None));
    }

    #[test]
    fn a_token_from_a_different_seat_is_rejected_even_if_otherwise_fresh() {
        assert!(!token_serial_is_fresh(
            false,
            Some(serial(5)),
            Some(serial(5))
        ));
    }

    #[test]
    fn a_token_no_older_than_the_last_keyboard_enter_is_fresh() {
        assert!(token_serial_is_fresh(
            true,
            Some(serial(5)),
            Some(serial(5))
        ));
        assert!(token_serial_is_fresh(
            true,
            Some(serial(7)),
            Some(serial(5))
        ));
    }

    #[test]
    fn a_token_older_than_the_last_keyboard_enter_is_stale() {
        assert!(!token_serial_is_fresh(
            true,
            Some(serial(3)),
            Some(serial(5))
        ));
    }

    #[test]
    fn a_token_at_exactly_the_age_limit_is_still_fresh_enough() {
        assert!(token_is_fresh_enough(MAX_TOKEN_AGE));
    }

    #[test]
    fn a_token_older_than_the_age_limit_is_rejected() {
        assert!(!token_is_fresh_enough(
            MAX_TOKEN_AGE + std::time::Duration::from_secs(1)
        ));
    }

    #[test]
    fn a_freshly_created_token_is_fresh_enough() {
        assert!(token_is_fresh_enough(std::time::Duration::ZERO));
    }
}
