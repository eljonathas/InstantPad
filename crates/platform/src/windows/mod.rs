//! Windows adapters. The calls into the system need `unsafe`, which `lib.rs` allows for this
//! module only.

mod vb_audio;

#[cfg(target_os = "windows")]
mod cable;
#[cfg(target_os = "windows")]
mod endpoints;
#[cfg(target_os = "windows")]
mod installer;
#[cfg(target_os = "windows")]
mod shell;
#[cfg(target_os = "windows")]
mod signature;

#[cfg(target_os = "windows")]
pub use cable::VbCable;
#[cfg(target_os = "windows")]
pub use endpoints::Endpoints;
