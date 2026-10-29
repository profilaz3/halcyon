//! Versioned applied widget state. The model is pure; the existing desktop
//! store owns activation, writes, history and recovery.
mod model;
pub use model::{Document, InstanceId};
pub(crate) mod commands;
pub(crate) mod legacy;
pub(crate) mod storage;

pub const FORMAT: &str = "halcyon-widget-design";
pub const VERSION: u32 = 3;
pub const MAX_BYTES: usize = 512 * 1024;
pub const MAX_INSTANCES: usize = 64;

#[cfg(test)]
mod tests;
