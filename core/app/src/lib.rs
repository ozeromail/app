//! Application layer: use cases built only on domain ports.

pub mod ops;
pub mod send;
pub mod sync;

#[cfg(test)]
mod fakes;
