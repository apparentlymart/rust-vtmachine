//! Virtual terminal state machine implementation.
//!
//! This library provides the lowest-level handling of a virtual terminal stream,
//! recognizing escape sequences and other control characters and delivering
//! them to a caller-provided handler.
//!
//! For example, given the sequence `"\x1b[10;10H"` this library can report that
//! this is a control sequence with function character `H` and the parameters
//! `[10, 10]`, but it's up to the provided handler to interpret that as a command
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
//! state machine and delivers events to an implementation of trait [`VtHandler`].
//!
//! ```rust
//! # use vtmachine::{VtEvent, VtMachine, VtParams, VtIntermediates, vt_handler_fn};
//! # use u8char::u8char;
//! # let mut evts: Vec<VtEvent> = Vec::new();
//! let mut machine = VtMachine::new(vt_handler_fn(|event| {
//!     println!("{event:?}");
//! #   evts.push(event);
//! }));
//! machine.write("\x1b[2J\x1b[1;1HHello!\r\n");
//! # drop(machine);
//! # assert_eq!(&evts[..], &[
//! #    VtEvent::DispatchCsi { cmd: b'J', params: VtParams::from_slice(&[2]), intermediates: VtIntermediates::new() },
//! #    VtEvent::DispatchCsi { cmd: b'H', params: VtParams::from_slice(&[1, 1]), intermediates: VtIntermediates::new() },
//! #    VtEvent::Print(u8char::from_char('H')),
//! #    VtEvent::Print(u8char::from_char('e')),
//! #    VtEvent::Print(u8char::from_char('l')),
//! #    VtEvent::Print(u8char::from_char('l')),
//! #    VtEvent::Print(u8char::from_char('o')),
//! #    VtEvent::Print(u8char::from_char('!')),
//! #    VtEvent::PrintEnd,
//! #    VtEvent::ExecuteCtrl(b'\r'),
//! #    VtEvent::ExecuteCtrl(b'\n'),
//! # ]);
//! ```
//!
//! ```plaintext
//! DispatchCsi { cmd: 'J', params: VtParams([2]), intermediates: VtIntermediates([]) }
//! DispatchCsi { cmd: 'H', params: VtParams([1, 1]), intermediates: VtIntermediates([]) }
//! Print('H')
//! Print('e')
//! Print('l')
//! Print('l')
//! Print('o')
//! Print('!')
//! PrintEnd
//! ExecuteCtrl('\r')
//! ExecuteCtrl('\n')
//! ```
#![no_std]

mod handler;
mod machine;

pub use handler::{vt_handler_fn, VtEvent, VtHandler};
pub use machine::{VtIntermediates, VtMachine, VtParams};

#[cfg(test)]
mod tests;
