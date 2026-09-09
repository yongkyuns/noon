//! Shared reactive, native-input and host-evaluation contracts and helpers.
//!
//! Mutable evaluation and effective frame state belong to `noon-runtime`.
//! Authored identity, declarations and transactions belong to the sibling
//! `semantic_store` module; immutable font/text/geometry resources, animation
//! plans and publication revision types have their own sibling modules. This
//! module must not aggregate those owners or expose a second semantic store.
//!
//! This describes current Phase A placement, not completion of the target
//! semantic/execution separation in `docs/architecture.md` section 13 (#960).

mod host_semantics;
pub use host_semantics::*;

mod native_reactive;
pub use native_reactive::*;

mod native_input_runtime;
pub use native_input_runtime::*;

mod native_inputs;
pub use native_inputs::*;
