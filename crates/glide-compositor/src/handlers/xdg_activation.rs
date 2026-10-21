use smithay::{
    input::keyboard::KeyboardHandle,
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    utils::Serial,
    wayland::xdg_activation::{XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData},
};

use crate::GlideState;

pub const MAX_TOKEN_AGE: std::time::Duration = std::time::Duration::from_secs(10);
pub const LAUNCH_INTENT: std::time::Duration = std::time::Duration::from_secs(5);
pub const USER_ACTION: std::time::Duration = std::time::Duration::from_secs(5);

pub fn user_pressed<I: smithay::backend::input::InputBackend>(event: &smithay::backend::input::InputEvent<I>) -> bool {
    use smithay::backend::input::{ButtonState, InputEvent, KeyState, KeyboardKeyEvent, PointerButtonEvent};
    match event {
        InputEvent::Keyboard { event } => event.state() == KeyState::Pressed,
        InputEvent::PointerButton { event } => event.state() == ButtonState::Pressed,
        InputEvent::TouchDown { .. }
        | InputEvent::GestureSwipeBegin { .. }
        | InputEvent::GesturePinchBegin { .. }
        | InputEvent::GestureHoldBegin { .. } => true,
        _ => false,
    }
}

#[must_use]
pub fn follows_user_action(last_input: Option<std::time::Instant>, token_created: std::time::Instant) -> bool {
    last_input
        .is_some_and(|input| input <= token_created && token_created.saturating_duration_since(input) <= USER_ACTION)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActivationGrant(bool);

pub fn grant(data: &XdgActivationTokenData) {
    data.user_data.insert_if_missing(|| ActivationGrant(true));
}

fn is_granted(data: &XdgActivationTokenData) -> bool {
    data.user_data.get::<ActivationGrant>().is_some_and(|grant| grant.0)
}

const fn activation_allowed(granted: bool, age: std::time::Duration) -> bool {
    granted && token_is_fresh_enough(age)
}

fn token_serial_is_fresh(
    seat_owns_requesting_seat: bool, token_serial: Option<Serial>, keyboard_last_enter: Option<Serial>,
) -> bool {
    seat_owns_requesting_seat
        && token_serial.zip(keyboard_last_enter).is_some_and(|(token, enter)| token.is_no_older_than(&enter))
}

fn requester_has_keyboard_focus<T: PartialEq>(requester: Option<T>, focused: Option<T>) -> bool {
    requester.zip(focused).is_some_and(|(r, f)| r == f)
}

const fn token_is_fresh_enough(age: std::time::Duration) -> bool {
    age.as_secs() <= MAX_TOKEN_AGE.as_secs()
}

impl XdgActivationHandler for GlideState {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.protocols.xdg_activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        let keyboard = self.seat.get_keyboard();
        let focused_surface = keyboard.as_ref().and_then(KeyboardHandle::current_focus);
        let focused_client = focused_surface.as_ref().and_then(Resource::client).map(|client| client.id());
        let requester = data
            .surface
            .as_ref()
            .and_then(Resource::client)
            .map(|client| client.id())
            .or_else(|| data.client_id.clone());
        let fresh = requester_has_keyboard_focus(requester, focused_client)
            && data.serial.as_ref().is_none_or(|(serial, seat)| {
                let last_enter = keyboard.as_ref().and_then(KeyboardHandle::last_enter);
                token_serial_is_fresh(self.seat.owns(seat), Some(*serial), last_enter)
            });
        data.user_data.insert_if_missing(|| ActivationGrant(fresh));
        self.protocols.xdg_activation_state.retain_tokens(|_, data| token_is_fresh_enough(data.timestamp.elapsed()));
        true
    }

    fn request_activation(
        &mut self, token: XdgActivationToken, token_data: XdgActivationTokenData, surface: WlSurface,
    ) {
        let Some(id) = self.window_id_for_surface(&surface) else {
            return;
        };
        let granted = activation_allowed(is_granted(&token_data), token_data.timestamp.elapsed());
        let focused = self.layout.focused_window() == Some(id);
        let fresh = token_is_fresh_enough(token_data.timestamp.elapsed());
        let reacting = !granted && fresh && follows_user_action(self.session.last_user_input, token_data.timestamp);
        let launched = !granted && !reacting && !focused && self.take_launch_intent();
        if granted || reacting || launched {
            self.protocols.xdg_activation_state.remove_token(&token);
            tracing::info!(window = id.0, granted, reacting, launched, "activation honoured");
            if self.layout.focus_window(id) {
                self.reconcile_space();
            }
            return;
        }
        if !focused {
            tracing::info!(window = id.0, "activation refused: the token was not granted and nothing was launched");
            self.demand_attention(id);
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
    fn a_token_minted_right_after_a_click_counts_as_the_users_doing() {
        let click = std::time::Instant::now();
        let token = click + std::time::Duration::from_millis(400);
        assert!(follows_user_action(Some(click), token));
        assert!(!follows_user_action(None, token), "no input yet means nobody asked for it");
        assert!(!follows_user_action(Some(token + std::time::Duration::from_millis(10)), token), "the user moved on");
        assert!(!follows_user_action(Some(click), click + USER_ACTION + std::time::Duration::from_millis(1)));
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
        assert!(!token_serial_is_fresh(false, Some(serial(5)), Some(serial(5))));
    }

    #[test]
    fn a_token_no_older_than_the_last_keyboard_enter_is_fresh() {
        assert!(token_serial_is_fresh(true, Some(serial(5)), Some(serial(5))));
        assert!(token_serial_is_fresh(true, Some(serial(7)), Some(serial(5))));
    }

    #[test]
    fn a_token_older_than_the_last_keyboard_enter_is_stale() {
        assert!(!token_serial_is_fresh(true, Some(serial(3)), Some(serial(5))));
    }

    #[test]
    fn only_the_client_holding_keyboard_focus_may_mint_a_focus_token() {
        assert!(requester_has_keyboard_focus(Some(1), Some(1)));
        assert!(
            !requester_has_keyboard_focus(Some(2), Some(1)),
            "a hovered background client has a fresh serial but no focus"
        );
        assert!(!requester_has_keyboard_focus(Some(1), None));
        assert!(!requester_has_keyboard_focus::<u8>(None, Some(1)));
    }

    #[test]
    fn a_token_at_exactly_the_age_limit_is_still_fresh_enough() {
        assert!(token_is_fresh_enough(MAX_TOKEN_AGE));
    }

    #[test]
    fn a_token_older_than_the_age_limit_is_rejected() {
        assert!(!token_is_fresh_enough(MAX_TOKEN_AGE + std::time::Duration::from_secs(1)));
    }

    #[test]
    fn a_freshly_created_token_is_fresh_enough() {
        assert!(token_is_fresh_enough(std::time::Duration::ZERO));
    }

    #[test]
    fn only_a_granted_fresh_token_may_steal_focus() {
        let now = std::time::Duration::ZERO;
        assert!(activation_allowed(true, now));
        assert!(!activation_allowed(false, now), "refused: it asks for attention");
        assert!(!activation_allowed(true, MAX_TOKEN_AGE + std::time::Duration::from_secs(1)));
    }

    #[test]
    fn a_compositor_launch_token_is_granted_and_a_bare_one_is_not() {
        let launched = XdgActivationTokenData::default();
        grant(&launched);
        assert!(is_granted(&launched));
        assert!(!is_granted(&XdgActivationTokenData::default()));
    }
}
