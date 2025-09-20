//! Virtual terminal state machine implementation.
//!
//! This library provides the lowest-level handling of a virtual terminal stream,
//! recognizing escape sequences and other control characters and returning
//! a series of events that should either write to the terminal buffer or
//! modify the terminal's higher-level state.
//!
//! For example, given the sequence `"\x1b[10;10H"` this library can report that
//! this is a control sequence with function character `H` and the parameters
//! `[10, 10]`, but it's up to the caller to interpret that as a command
//! to move the cursor to row 10, column 10.
//!
//! As with so many libraries like this, the state machine is based on the
//! work of [Paul Flo Williams](https://hisdeedsaredust.com/) in
//! [A parser for DEC’s ANSI-compatible video terminals](https://vt100.net/emu/dec_ansi_parser),
//! though any flaws are mine. This implementation does not aim to be fully
//! compatible with VT100 or its successors. In particular, it implements a
//! Unicode-native machine that does not support legacy character sets.
//!
//! The main entry point in this crate is [`VtMachine`], which implements the
//! state machine. Pass each new character to [`VtMachine::write_u8char`], which
//! then returns a series of events that the character caused.
//!
//! If you are recieving the terminal data as a byte stream (likely!) then
//! you can use [`::u8char::stream::U8CharStream`] to translate the incoming
//! bytes into a series of [`u8char`] values to pass to your `VtMachine`. The
//! "report" example demonstrates that usage pattern.
#![no_std]

mod machine;

pub use machine::{VtEvent, VtMachine};

#[cfg(test)]
mod tests;
