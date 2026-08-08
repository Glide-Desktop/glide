use std::sync::Arc;

use smithay::wayland::security_context::{
    SecurityContext, SecurityContextHandler, SecurityContextListenerSource,
};

use crate::{GlideState, state::ClientState};

impl SecurityContextHandler for GlideState {
    fn context_created(
        &mut self,
        source: SecurityContextListenerSource,
        security_context: SecurityContext,
    ) {
        let result = self
            .loop_handle
            .insert_source(source, move |stream, (), state| {
                let client_state = ClientState {
                    security_context: Some(security_context.clone()),
                    ..ClientState::default()
                };
                if let Err(err) = state
                    .display_handle
                    .insert_client(stream, Arc::new(client_state))
                {
                    tracing::warn!(?err, "failed to accept a sandboxed client connection");
                }
            });
        if let Err(err) = result {
            tracing::warn!(
                ?err,
                "failed to register a security-context listener socket"
            );
        }
    }
}
