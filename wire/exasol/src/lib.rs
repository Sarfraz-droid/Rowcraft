//! Exasol WebSocket JSON API.
//!
//! Clean-room implementation from published specs and recorded traffic (see SPEC.md §5).
//! Knows nothing about dialects: exposes typed messages and a client state machine.
