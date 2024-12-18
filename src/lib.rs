#![no_std]

mod handler;
mod machine;

pub use handler::{vt_handler_fn, VtEvent, VtHandler};
pub use machine::{VtIntermediates, VtMachine, VtParams};

#[cfg(test)]
mod tests;
