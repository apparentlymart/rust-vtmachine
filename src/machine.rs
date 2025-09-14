use crate::VtHandler;
use u8char::u8char;

/// Virtual terminal state machine.
///
/// This is the main type in this crate, which takes Unicode scalar values (or strings thereof)
/// and translates them into low-level events to be interpreted by a provided [`VtHandler`].
///
/// `VtMachine` implements a _Unicode-native_ terminal state machine that does not support
/// any legacy character encodings. If working with a raw byte stream, such as from a
/// pseudoterminal provided by the host OS, the caller must first interpret the bytes
/// as UTF-8 sequences and provide the result to either [`VtMachine::write`] or
/// [`VtMachine::write_char`], depending on the granularity of the UTF-8 interpretation.
///
/// This implementation is not suitable for emulating a legacy hardware video terminal
/// that used switchable character sets.
pub struct VtMachine<H> {
    handler: H,
    state: State,
    intermediates: VtIntermediates,
    params: VtParams,
    in_literal_chunk: bool,
}

impl<H> VtMachine<H> {
    /// Constructs a new [`VtMachine`] that will deliver events to the given [`VtHandler`].
    pub const fn new(handler: H) -> Self {
        Self {
            handler,
            state: State::Literal,
            intermediates: VtIntermediates::new(),
            params: VtParams::new(),
            in_literal_chunk: false,
        }
    }

    /// Returns a shared reference to the wrapped [`VtHandler`].
    #[inline(always)]
    pub const fn handler(&self) -> &H {
        &self.handler
    }

    /// Returns a mutable reference to the wrapped [`VtHandler`].
    #[inline(always)]
    pub const fn handler_mut(&mut self) -> &mut H {
        &mut self.handler
    }

    /// Consumes the [`VtMachine`] and returns ownership of its wrapped [`VtHandler`].
    #[inline(always)]
    pub fn take_handler(self) -> H {
        self.handler
    }
}

impl<H: VtHandler> VtMachine<H> {
    /// Consumes each of the unicode scalar values in the given string, interpreting
    /// any control characters to produce special events such as control sequences.
    ///
    /// Note that this requires the buffer to be [`str`], meaning it's assumed
    /// to be valid UTF-8. If you're consuming a stream of [`u8`] then you
    /// might instead consider using [`::u8char::stream::U8CharStream`] and
    /// passing the [`u8char`] values that its iterators produce directly into
    /// [`Self::write_u8char`]. (Note that `U8CharStream` is lossy when given
    /// invalid UTF-8 as input, though.)
    pub fn write(&mut self, data: &str) {
        use ::u8char::AsU8Chars;
        for c in data.u8chars() {
            self.write_u8char(c);
        }
    }

    /// Consumes a single unicode scalar value given as a [`u8char`], in the
    /// same way as [`Self::write`] would consume each scalar value its the
    /// given string.
    pub fn write_u8char(&mut self, c: u8char) {
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
                    return self.action(Action::Execute, c);
                }
                _ => return self.action(Action::Print, c),
            },
            State::Escape => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.action(Action::Execute, c);
                }
                b'\x7f' => {
                    return; // Ignored
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
                    return self.action(Action::Execute, c);
                }
                b'\x7f' => {
                    return; // Ignored
                }
                b'\x20'..=b'\x2f' => {
                    return self.action(Action::Collect, c);
                }
                b'\x30'..=b'\x7e' => {
                    return self.change_state(State::Literal, Action::EscDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlStart => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return self.action(Action::Execute, c);
                }
                b'\x7f' => {
                    return; // Ignored
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
                    return self.action(Action::Execute, c);
                }
                b'\x30'..=b'\x39' | b'\x3b' => {
                    return self.action(Action::Param, c);
                }
                b'\x7f' => {
                    return; // Ignored
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
                    return self.action(Action::Execute, c);
                }
                b'\x20'..=b'\x2f' => {
                    return self.action(Action::Collect, c);
                }
                b'\x7f' => {
                    return; // Ignored
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
                    return self.action(Action::Execute, c);
                }
                b'\x20'..=b'\x3f' | b'\x7f' => {
                    return; // Ignored
                }
                b'\x40'..=b'\x7e' => {
                    return self.change_state(State::Literal, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlStart => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x7f' => {
                    return; // Ignored
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
                    return; // Ignored
                }
                b'\x30'..=b'\x39' | b'\x3b' => {
                    return self.action(Action::Param, c);
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
                    return; // Ignored
                }
                b'\x20'..=b'\x2f' => {
                    return self.action(Action::Collect, c);
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
                    return self.action(Action::Put, c);
                }
                b'\x7f' => {
                    return; // Ignored
                }
                _ => return self.error(c),
            },
            State::DevCtrlMalformed => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x20'..=b'\x7f' => {
                    return; // Ignored
                }
                _ => return self.error(c),
            },
            State::OsCmd => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' => {
                    return; // Ignored
                }
                b'\x20'..=b'\x7f' => {
                    return self.action(Action::OscPut, c);
                }
                _ => return self.error(c),
            },
            State::IgnoreUntilSt => match fb {
                b'\x00'..=b'\x17' | b'\x19' | b'\x1c'..=b'\x1f' | b'\x20'..=b'\x7f' => {
                    return; // Ignored
                }
                _ => return self.error(c),
            },
        }
    }

    /// Consumes a single unicode scalar value given as a [`char`], in the same way
    /// as [`Self::write`] would consume each scalar value its the given string.
    ///
    /// Note that [`VtMachine`] uses [`u8char`] as its primary representation
    /// of characters, and so this function is really just converting the given
    /// `char` to `u8char` and then passing it to [`Self::write_u8char`]. If
    /// you already have a `u8char` value then it's better to use the other
    /// function directly.
    pub fn write_char(&mut self, c: char) {
        self.write_u8char(u8char::from_char(c))
    }

    /// Tells the [`VtMachine`] that no more bytes are expected, such as if
    /// the stream that the data is arriving from is closed from the writer
    /// end.
    ///
    /// This notifies the handler of the end of any currently-active literal
    /// chunk and then resets the machine back to its initial state. It's
    /// okay to keep using the [`VtMachine`] after calling this function, but
    /// any subsequent character written will be treated as if it is the first
    /// character in a new stream.
    pub fn write_end(&mut self) {
        if self.in_literal_chunk {
            self.in_literal_chunk = false;
            self.handler.print_end();
        }
        self.state = State::Literal;
        self.intermediates.clear();
        self.params.clear();
    }

    fn action(&mut self, action: Action, c: u8char) {
        if matches!(action, Action::Print) {
            self.in_literal_chunk = true;
        } else if self.in_literal_chunk {
            self.in_literal_chunk = false;
            self.handler.print_end();
        }
        match action {
            Action::Print => self.handler.print(c),
            Action::Execute => self.handler.execute_ctrl(c.first_byte()),
            Action::Hook => {
                self.handler
                    .dcs_start(c.first_byte(), &self.params, &self.intermediates)
            }
            Action::Put => self.handler.dcs_char(c),
            Action::OscStart => self.handler.osc_start(c.first_byte()),
            Action::OscPut => self.handler.osc_char(c),
            Action::OscEnd => self.handler.osc_end(c.first_byte()),
            Action::Unhook => self.handler.dcs_end(c.first_byte()),
            Action::CsiDispatch => {
                self.handler
                    .dispatch_csi(c.first_byte(), &self.params, &self.intermediates);
            }
            Action::EscDispatch => self
                .handler
                .dispatch_esc(c.first_byte(), &self.intermediates),
            Action::None => {}
            Action::Collect => self.intermediates.push(c.first_byte()),
            Action::Param => {
                self.params.push_csi_char(c);
            }
            Action::Clear => {
                self.intermediates.clear();
                self.params.clear();
            }
        }
    }

    fn change_state(&mut self, state: State, transition: Action, c: u8char) {
        self.state_exit_actions(self.state, c);
        self.state = state;
        self.action(transition, c);
        self.state_entry_actions(state, c);
    }

    fn state_entry_actions(&mut self, state: State, c: u8char) {
        match state {
            State::Escape => self.action(Action::Clear, c),
            State::CtrlStart => self.action(Action::Clear, c),
            State::DevCtrlStart => self.action(Action::Clear, c),
            State::OsCmd => self.action(Action::OscStart, c),
            State::DevCtrlPassthru => self.action(Action::Hook, c),
            _ => {}
        }
    }

    fn state_exit_actions(&mut self, state: State, c: u8char) {
        match state {
            State::OsCmd => self.action(Action::OscEnd, c),
            State::DevCtrlPassthru => self.action(Action::Unhook, c),
            _ => {}
        }
    }

    fn error(&mut self, c: u8char) {
        self.handler.error(c);
        self.change_state(State::Literal, Action::None, c);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Print,
    Execute,
    Hook,
    Put,
    OscStart,
    OscPut,
    OscEnd,
    Unhook,
    CsiDispatch,
    EscDispatch,
    None,
    Collect,
    Param,
    Clear,
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
