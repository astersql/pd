// Copyright 2026 AsterSQL.
//! Shared MCS listener ownership over the caller's executor and transport.
mod base_listener;
mod cancellable_io;
pub use base_listener::{BaseServer, StartCallback};
#[cfg(test)]
mod base_listener_test;
mod delegate_connections;
pub use delegate_connections::{ClientTlsFactory, DelegateChannel, DelegateConnections};
#[cfg(test)]
mod delegate_connections_test;

mod primary_observer;
pub use primary_observer::PrimaryObserver;
#[cfg(test)]
mod primary_observer_test;

mod auxiliary;
mod diagnostics_info;
mod diagnostics_log;
mod go_pattern;
pub use auxiliary::auxiliary_routes;
pub use diagnostics_log::DiagnosticsService;
pub use go_pattern::go_pattern;
#[cfg(test)]
mod auxiliary_test;

mod owned_channel;
pub use owned_channel::OwnedDelegateChannel;
