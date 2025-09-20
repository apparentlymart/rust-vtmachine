use core::mem::MaybeUninit;

use u8char::u8char;

/// Virtual terminal state machine.
///
/// This is the main type in this crate, which takes Unicode scalar values
/// and translates them into low-level events to be interpreted by a
/// higher-level terminal emulator implementation.
///
/// `VtMachine` implements a _Unicode-native_ terminal state machine that does
/// not support any legacy character encodings. If working with a raw byte
/// stream, such as from a pseudoterminal provided by the host OS, the caller
/// must first interpret the bytes as UTF-8 sequences and provide the result to
/// either [`VtMachine::write_u8char`].
///
/// This implementation is not suitable for emulating a legacy hardware video
/// terminal that used switchable character sets.
pub struct VtMachine {
    state: State,
    intermediates: VtIntermediates,
    params: VtParams,
    in_literal_chunk: bool,
}

impl VtMachine {
    /// Constructs a new [`VtMachine`].
    pub const fn new() -> Self {
        Self {
            state: State::Literal,
            intermediates: VtIntermediates::new(),
            params: VtParams::new(),
            in_literal_chunk: false,
        }
    }

    /// Consumes a single unicode scalar value given as a [`u8char`], returning
    /// a series of events that the character causes.
    ///
    /// The caller should consume the entire iterator in order to stay properly
    /// synchronized with the `VtMachine`.
    pub fn write_u8char<'m>(&'m mut self, c: u8char) -> impl Iterator<Item = VtEvent<'m>> {
        // All of the special state transitions and actions are triggered by
        // bytes in the ASCII range, so we will match those based on only the
        // first byte of the UTF-8 character. For values less than 128 these
        // bytes will be the whole represented character, and we're not going
        // to match any values >=128.
        let fb = c.first_byte();

        // Some characters have the same effect regardless of the current state.
        match fb {
            b'\x18' | b'\x1a' | b'\x80'..=b'\x8f' | b'\x91'..=b'\x97' | b'\x99' | b'\x9a' => {
                return self.change_state(State::Literal, Action::Execute, c);
            }
            b'\x9c' => {
                return self.change_state(State::Literal, Action::None, c);
            }
            b'\x1b' => {
                return self.change_state(State::Escape, Action::None, c);
            }
            b'\x98' | b'\x9e' | b'\x9f' => {
                return self.change_state(State::IgnoreUntilSt, Action::None, c);
            }
            b'\x90' => {
                return self.change_state(State::DevCtrlStart, Action::None, c);
            }
            b'\x9d' => {
                return self.change_state(State::OsCmd, Action::None, c);
            }
            b'\x9b' => {
                return self.change_state(State::CtrlStart, Action::None, c);
            }
            _ => {
                // We'll continue below for any other character.
            }
        }

        // For any character that doesn't have a universal handling above,
        // we vary based on state.
        match self.state {
            State::Literal => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.just_action(Action::Execute, c);
                }
                _ => return self.just_action(Action::Print, c),
            },
            State::Escape => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.just_action(Action::Execute, c);
                }
                b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x20'..=b'\x2f' => {
                    return self.change_state(State::EscapeIntermediate, Action::Collect, c);
                }
                b'\x30'..=b'\x4f'
                | b'\x51'..=b'\x57'
                | b'\x59'
                | b'\x5a'
                | b'\x5c'
                | b'\x60'..=b'\x7e' => {
                    return self.change_state(State::Literal, Action::EscDispatch, c);
                }
                b'\x5b' => {
                    return self.change_state(State::CtrlStart, Action::None, c);
                }
                b'\x5d' => {
                    return self.change_state(State::OsCmd, Action::None, c);
                }
                b'\x50' => {
                    return self.change_state(State::DevCtrlStart, Action::None, c);
                }
                b'\x58' | b'\x5e' | b'\x5f' => {
                    return self.change_state(State::IgnoreUntilSt, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::EscapeIntermediate => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.just_action(Action::Execute, c);
                }
                b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x20'..=b'\x2f' => {
                    return self.just_action(Action::Collect, c);
                }
                b'\x30'..=b'\x7e' => {
                    return self.change_state(State::Literal, Action::EscDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlStart => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.just_action(Action::Execute, c);
                }
                b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x20'..=b'\x2f' => {
                    return self.change_state(State::CtrlIntermediate, Action::Collect, c);
                }
                b'\x3a' => {
                    return self.change_state(State::CtrlMalformed, Action::None, c);
                }
                b'\x30'..=b'\x39' | b'\x3b' => {
                    return self.change_state(State::CtrlParam, Action::Param, c);
                }
                b'\x3c'..=b'\x3f' => {
                    return self.change_state(State::CtrlParam, Action::Collect, c);
                }
                b'\x40'..=b'\x7e' => {
                    return self.change_state(State::Literal, Action::CsiDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlParam => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.just_action(Action::Execute, c);
                }
                b'\x30'..=b'\x39' | b'\x3b' => {
                    return self.just_action(Action::Param, c);
                }
                b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x3a' | b'\x3c'..=b'\x3f' => {
                    return self.change_state(State::CtrlMalformed, Action::None, c);
                }
                b'\x20'..=b'\x2f' => {
                    return self.change_state(State::CtrlIntermediate, Action::Collect, c);
                }
                b'\x40'..=b'\x7e' => {
                    return self.change_state(State::Literal, Action::CsiDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlIntermediate => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.just_action(Action::Execute, c);
                }
                b'\x20'..=b'\x2f' => {
                    return self.just_action(Action::Collect, c);
                }
                b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x3a' | b'\x3c'..=b'\x3f' => {
                    return self.change_state(State::CtrlMalformed, Action::None, c);
                }
                b'\x40'..=b'\x7e' => {
                    return self.change_state(State::Literal, Action::CsiDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlMalformed => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.just_action(Action::Execute, c);
                }
                b'\x20'..=b'\x3f' | b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x40'..=b'\x7e' => {
                    return self.change_state(State::Literal, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlStart => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x3a' => {
                    return self.change_state(State::DevCtrlMalformed, Action::None, c);
                }
                b'\x20'..=b'\x2f' => {
                    return self.change_state(State::DevCtrlIntermediate, Action::Collect, c);
                }
                b'\x30'..=b'\x39' | b'\x3b' => {
                    return self.change_state(State::DevCtrlParam, Action::Param, c);
                }
                b'\x3c'..=b'\x3f' => {
                    return self.change_state(State::DevCtrlParam, Action::Collect, c);
                }
                b'\x40'..=b'\x7e' => {
                    return self.change_state(State::DevCtrlPassthru, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlParam => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x30'..=b'\x39' | b'\x3b' => {
                    return self.just_action(Action::Param, c);
                }
                b'\x3a' | b'\x3c'..=b'\x3f' => {
                    return self.change_state(State::DevCtrlMalformed, Action::None, c);
                }
                b'\x20'..=b'\x2f' => {
                    return self.change_state(State::DevCtrlIntermediate, Action::Collect, c);
                }
                b'\x40'..=b'\x7e' => {
                    return self.change_state(State::DevCtrlPassthru, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlIntermediate => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                b'\x20'..=b'\x2f' => {
                    return self.just_action(Action::Collect, c);
                }
                b'\x30'..=b'\x3f' => {
                    return self.change_state(State::DevCtrlMalformed, Action::None, c);
                }
                b'\x40'..=b'\x7e' => {
                    return self.change_state(State::DevCtrlPassthru, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlPassthru => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x20'..=b'\x7e' => {
                    return self.just_action(Action::Put, c);
                }
                b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                _ => return self.error(c),
            },
            State::DevCtrlMalformed => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x20'..=b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                _ => return self.error(c),
            },
            State::OsCmd => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.no_change(); // Ignored
                }
                b'\x20'..=b'\x7f' => {
                    return self.just_action(Action::OscPut, c);
                }
                _ => return self.error(c),
            },
            State::IgnoreUntilSt => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x20'..=b'\x7f' => {
                    return self.no_change(); // Ignored
                }
                _ => return self.error(c),
            },
        }
    }

    /// Consumes a single unicode scalar value given as a [`char`].
    ///
    /// Note that [`VtMachine`] uses [`u8char`] as its primary representation
    /// of characters, and so this function is really just converting the given
    /// `char` to `u8char` and then passing it to [`Self::write_u8char`]. If
    /// you already have a `u8char` value then it's better to use the other
    /// function directly.
    pub fn write_char<'m>(&'m mut self, c: char) -> impl Iterator<Item = VtEvent<'m>> {
        self.write_u8char(u8char::from_char(c))
    }

    /// Tells the [`VtMachine`] that no more bytes are expected, such as if
    /// the stream that the data is arriving from is closed from the writer
    /// end.
    ///
    /// This can potentially return some final events caused by ending sequences
    /// that had not yet been explicitly terminated.
    ///
    /// It's okay to keep using the [`VtMachine`] after calling this function,
    /// but any subsequent character written will be treated as if it is the
    /// first character in a new stream.
    pub fn write_end(&mut self) -> impl Iterator<Item = VtEvent<'static>> {
        self.state = State::Literal;
        self.intermediates.clear();
        self.params.clear();
        let event = if self.in_literal_chunk {
            self.in_literal_chunk = false;
            Some(VtEvent::PrintEnd)
        } else {
            None
        };
        Transition::new([event])
    }

    fn action(&mut self, action: Action, c: u8char) -> Option<VtEvent<'static>> {
        match action {
            Action::Collect => self.intermediates.push(c.first_byte()),
            Action::Param => {
                self.params.push_csi_char(c);
            }
            Action::Clear | Action::Error => {
                self.intermediates.clear();
                self.params.clear();
            }
            Action::Print => {}
            Action::Execute => {}
            Action::Hook => {}
            Action::Put => {}
            Action::OscStart => {}
            Action::OscPut => {}
            Action::CsiDispatch => {}
            Action::EscDispatch => {}
            Action::None => {}
        }
        if matches!(action, Action::Print) {
            self.in_literal_chunk = true;
        } else if self.in_literal_chunk {
            self.in_literal_chunk = false;
            return Some(VtEvent::PrintEnd);
        }
        None
    }

    fn action_event<'m>(&'m self, action: Action, c: u8char) -> Option<VtEvent<'m>> {
        match action {
            Action::Print => Some(VtEvent::Print(c)),
            Action::Execute => Some(VtEvent::ExecuteCtrl(c.first_byte())),
            Action::Hook => Some(VtEvent::DcsStart {
                cmd: c.first_byte(),
                params: &self.params,
                intermediates: &self.intermediates,
            }),
            Action::Put => Some(VtEvent::DcsChar(c)),
            Action::OscStart => Some(VtEvent::OscStart(c.first_byte())),
            Action::OscPut => Some(VtEvent::OscChar(c)),
            Action::CsiDispatch => Some(VtEvent::DispatchCsi {
                cmd: c.first_byte(),
                params: &self.params,
                intermediates: &self.intermediates,
            }),
            Action::EscDispatch => Some(VtEvent::DispatchEsc {
                cmd: c.first_byte(),
                intermediates: &self.intermediates,
            }),
            Action::None => None,
            Action::Collect => None,
            Action::Param => None,
            Action::Clear => None,
            Action::Error => Some(VtEvent::Error(c)),
        }
    }

    fn just_action<'m>(&'m mut self, action: Action, c: u8char) -> Transition<'m> {
        let main_cleanup_event = self.action(action, c);
        let main_event = self.action_event(action, c);
        Transition::new([main_cleanup_event, main_event])
    }

    fn no_change(&self) -> Transition<'static> {
        Transition::new([])
    }

    fn change_state<'m>(
        &'m mut self,
        state: State,
        transition: Action,
        c: u8char,
    ) -> Transition<'m> {
        let exit_event = self.state_exit_event(self.state, c);
        self.state = state;

        let entry_action = self.state_entry_action(state);
        let entry_cleanup_event = if let Some(action) = entry_action {
            self.action(action, c)
        } else {
            None
        };
        let main_cleanup_event = self.action(transition, c);
        let entry_event = if let Some(action) = entry_action {
            self.action_event(action, c)
        } else {
            None
        };
        let main_event = self.action_event(transition, c);

        Transition::new([
            exit_event,
            entry_cleanup_event,
            main_cleanup_event,
            main_event,
            entry_event,
        ])
    }

    fn state_entry_action(&mut self, state: State) -> Option<Action> {
        match state {
            State::Escape => Some(Action::Clear),
            State::CtrlStart => Some(Action::Clear),
            State::DevCtrlStart => Some(Action::Clear),
            State::OsCmd => Some(Action::OscStart),
            State::DevCtrlPassthru => Some(Action::Hook),
            _ => None,
        }
    }

    fn state_exit_event(&mut self, state: State, c: u8char) -> Option<VtEvent<'static>> {
        match state {
            State::OsCmd => Some(VtEvent::OscEnd(c.first_byte())),
            State::DevCtrlPassthru => Some(VtEvent::DcsEnd(c.first_byte())),
            _ => None,
        }
    }

    fn error<'m>(&'m mut self, c: u8char) -> Transition<'m> {
        self.change_state(State::Literal, Action::Error, c)
    }
}

/// Our iterator type for events caused by writing a new character.
///
/// This is a stack-allocated fixed-size buffer for up to five events,
/// as a compromise to avoid a heap allocation for each new character, since
/// we need a separate object to represent our potential borrow of data
/// from inside the `VtMachine`.
struct Transition<'m> {
    next: usize,
    events: [MaybeUninit<VtEvent<'m>>; 5],
}

impl<'m> Transition<'m> {
    #[inline(always)]
    pub fn new<const N: usize>(events: [Option<VtEvent<'m>>; N]) -> Self {
        assert!(const { N <= 5 });
        let mut ret = Self {
            next: 5,
            events: [MaybeUninit::uninit(); 5],
        };
        for maybe_event in events.iter().rev() {
            if let Some(event) = maybe_event {
                ret.next -= 1;
                ret.events[ret.next].write(*event);
            }
        }
        ret
    }
}

impl<'m> Iterator for Transition<'m> {
    type Item = VtEvent<'m>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next == self.events.len() {
            return None;
        }
        let ret = unsafe { self.events[self.next].assume_init() };
        self.next += 1;
        Some(ret)
    }
}

/// An event from [`VtMachine`].
///
/// Some event types include borrowed values from inside the `VtMachine`'s
/// mutable state, and so all events must be dropped before writing another
/// character to the machine.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtEvent<'m> {
    /// Print a literal character at the current cursor position.
    Print(u8char),
    /// Emitted at the end of a series of consecutive [`VtEvent::Print`]
    /// events before emitting any other event, so that a terminal that
    /// is attempting to handle Unicode grapheme clusters can treat the
    /// transition points as "end-of-text" to reset the segmentation state
    /// machine.
    PrintEnd,
    /// Execute an appropriate action for the given control character.
    ExecuteCtrl(u8),
    /// Execute an appropriate action for the given control sequence.
    ///
    /// This is for sequence starting with the control sequence introducer,
    /// `ESC[`, and terminated with the byte given in `cmd`.
    DispatchCsi {
        /// The symbol at the end of the sequence representing the command
        /// to perform.
        cmd: u8,
        /// The semicolon-separated integer parameters.
        params: &'m VtParams,
        /// Any intermediate characters that appeared inside the sequence.
        intermediates: &'m VtIntermediates,
    },
    DispatchEsc {
        cmd: u8,
        intermediates: &'m VtIntermediates,
    },
    /// Reports the beginning of a device control string.
    ///
    /// Events of this type are followed by zero or more [`VtEvent::DcsChar`]
    /// and then one [`VtEvent::DcsEnd`], when the input stream is valid.
    DcsStart {
        cmd: u8,
        params: &'m VtParams,
        intermediates: &'m VtIntermediates,
    },
    /// Reports a literal character from within the "data string" portion of
    /// a device control string sequence.
    DcsChar(u8char),
    /// Marks the end of a device control string, reporting the character that
    /// ended it, which should be the "string terminator" character.
    DcsEnd(u8),
    /// Reports the beginning of an operating system command.
    ///
    /// Events of this type are followed by zero or more [`VtEvent::OscChar`]
    /// and then one [`VtEvent::OscEnd`], when the input stream is valid.
    OscStart(u8),
    /// Reports a literal character from within an operating system command.
    OscChar(u8char),
    /// Marks the end of an operating system command, reporting the character
    /// that ended it.
    OscEnd(u8),
    /// Emitted whenever the state machine encounters a character that is
    /// not expected in its current state.
    Error(u8char),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Print,
    Execute,
    Hook,
    Put,
    OscStart,
    OscPut,
    CsiDispatch,
    EscDispatch,
    None,
    Collect,
    Param,
    Clear,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Literal,
    Escape,
    EscapeIntermediate,
    CtrlStart,
    CtrlParam,
    CtrlIntermediate,
    CtrlMalformed,
    DevCtrlStart,
    DevCtrlParam,
    DevCtrlIntermediate,
    DevCtrlPassthru,
    DevCtrlMalformed,
    OsCmd,
    IgnoreUntilSt,
}

/// Zero or more `u16` values given as parameters in a control sequence, or similar.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VtParams {
    buf: [u16; 16],
    len: u8,
}

impl VtParams {
    /// Constructs a new zero-length [`VtParams`].
    pub const fn new() -> Self {
        Self {
            buf: [0; 16],
            len: 0,
        }
    }

    /// Constructs a new [`VtParams`] containing the values in the given slice.
    ///
    /// A `VtParams` has a maximum capacity of 16 items, so this will panic if
    /// the given slice has length 17 or greater.
    pub fn from_slice(from: &[u16]) -> Self {
        let mut ret = Self::new();
        if from.len() > ret.buf.len() {
            panic!("too many params")
        }
        ret.len = from.len() as u8;
        (&mut ret.buf[..from.len()]).copy_from_slice(from);
        ret
    }

    /// Attempts to push a new value.
    ///
    /// A [`VtParams`] has a capacity of 16 items, and so any pushes after
    /// that capacity has been reached are silently ignored.
    pub fn push(&mut self, v: u16) {
        if (self.len as usize) == self.buf.len() {
            return; // pushes beyond capacity are silently ignored
        }
        self.buf[self.len as usize] = v;
        self.len += 1;
    }

    fn push_csi_char(&mut self, c: u8char) {
        if c.first_byte() == b';' {
            // Argument separator, so we start a new param.
            self.push(0);
        } else {
            // The character must be a digit, then
            if self.len == 0 {
                self.push(0); // start our first param
            }
            let current = &mut self.buf[(self.len as usize) - 1];
            let digit = (c.to_char() as u16) - ('0' as u16);
            *current *= 10;
            *current += digit;
        }
    }

    /// Discard all of the parameters, causing the object to then have length zero.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Returns the parameter values as a slice of [`u16`] values.
    #[inline(always)]
    pub fn values(&self) -> &[u16] {
        &self.buf[..(self.len as usize)]
    }

    /// Returns the current number of parameters.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.len as usize
    }
}

impl core::fmt::Debug for VtParams {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("VtParams")
            .field(&&self.buf[..(self.len as usize)])
            .finish()
    }
}

/// Zero or more intermediate characters that appeared as part of an
/// escape sequence.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VtIntermediates {
    buf: [u8; 2],
    len: u8, // greater than length of buf means overrun
}

impl VtIntermediates {
    const OVERRUN_LEN: usize = 3;

    /// Constructs a new zero-length [`VtIntermediates`].
    pub const fn new() -> Self {
        Self {
            buf: [0; 2],
            len: 0,
        }
    }

    /// Constructs a new [`VtIntermediates`] containing the values in the given slice.
    ///
    /// A `VtIntermediates` has a maximum capacity of two items, so this will panic if
    /// the given slice has length three or greater.
    pub fn from_slice(from: &[u8]) -> Self {
        let mut ret = Self::new();
        if from.len() > ret.buf.len() {
            panic!("too many intermediates")
        }
        ret.len = from.len() as u8;
        (&mut ret.buf[..from.len()]).copy_from_slice(from);
        ret
    }

    /// Attempts to push a new value.
    ///
    /// A [`VtParams`] has a capacity of two characters, and so any pushes after
    /// that capacity has been reached are silently ignored.
    pub fn push(&mut self, c: u8) {
        let len = self.len();
        if len >= self.buf.len() {
            self.len = Self::OVERRUN_LEN as u8;
            return;
        }
        self.buf[len] = c;
        self.len += 1;
    }

    /// Discard all of the intermediate characters, causing the object to then have
    /// length zero.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Returns the intermediate characters as a slice of [`u8`] values.
    pub fn chars(&self) -> &[u8] {
        let len = self.len();
        &self.buf[..len]
    }

    /// Returns the current number of intermediate characters.
    #[inline(always)]
    pub fn len(&self) -> usize {
        core::cmp::min(self.buf.len(), self.len as usize)
    }

    /// Returns true if callers have attempted to push more than two intermediate
    /// characters, and thus subsequent characters have been discarded.
    #[inline(always)]
    pub const fn has_overrun(&self) -> bool {
        self.len as usize > self.buf.len()
    }
}

impl core::fmt::Debug for VtIntermediates {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("VtIntermediates")
            .field(&&self.buf[..(self.len as usize)])
            .finish()
    }
}
