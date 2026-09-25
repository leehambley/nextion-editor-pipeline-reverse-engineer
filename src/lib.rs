//! Decode and patch Nextion `.HMI` project files and compiled `.tft`
//! firmware — for the **NX8048P050-011R-Y display only**.
//!
//! This is not a general Nextion compiler. See `docs/targets.md` for what
//! that would take and why only one target is supported today, and
//! `README.md` for the overall project scope and its honest limitations
//! (no font pipeline, text-type components only for color/font patching,
//! same-length-only `.HMI` patches).

pub mod error;
pub mod hmi;
pub mod html;
pub mod spec;
pub mod target;
pub mod tft;

pub use error::{HmiError, SpecError, TftError};
pub use target::Target;
