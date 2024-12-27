use crate::VtHandler;

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
}

impl<H> VtMachine<H> {
    /// Constructs a new [`VtMachine`] that will deliver events to the given [`VtHandler`].
    pub const fn new(handler: H) -> Self {
        Self {
            handler,
            state: State::Literal,
            intermediates: VtIntermediates::new(),
            params: VtParams::new(),
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
    pub fn write(&mut self, data: &str) {
        for c in data.chars() {
            self.write_char(c);
        }
    }

    /// Consumes a single unicode scalar value given as a [`char`], in the same way
    /// as [`Self::write`] would consume each scalar value its the given string.
    pub fn write_char(&mut self, c: char) {
        // Some characters have the same effect regardless of the current state.
        match c {
            '\u{18}'
            | '\u{1a}'
            | '\u{80}'..='\u{8f}'
            | '\u{91}'..='\u{97}'
            | '\u{99}'
            | '\u{9a}' => {
                return self.change_state(State::Literal, Action::Execute, c);
            }
            '\u{9c}' => {
                return self.change_state(State::Literal, Action::None, c);
            }
            '\u{1b}' => {
                return self.change_state(State::Escape, Action::None, c);
            }
            '\u{98}' | '\u{9e}' | '\u{9f}' => {
                return self.change_state(State::IgnoreUntilSt, Action::None, c);
            }
            '\u{90}' => {
                return self.change_state(State::DevCtrlStart, Action::None, c);
            }
            '\u{9d}' => {
                return self.change_state(State::OsCmd, Action::None, c);
            }
            '\u{9b}' => {
                return self.change_state(State::CtrlStart, Action::None, c);
            }
            _ => {
                // We'll continue below for any other character.
            }
        }

        // For any character that doesn't have a universal handling above,
        // we vary based on state.
        match self.state {
            State::Literal => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' => {
                    return self.action(Action::Execute, c);
                }
                _ => return self.action(Action::Print, c),
            },
            State::Escape => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' => {
                    return self.action(Action::Execute, c);
                }
                '\u{7f}' => {
                    return; // Ignored
                }
                '\u{20}'..='\u{2f}' => {
                    return self.change_state(State::EscapeIntermediate, Action::Collect, c);
                }
                '\u{30}'..='\u{4f}'
                | '\u{51}'..='\u{57}'
                | '\u{59}'
                | '\u{5a}'
                | '\u{5c}'
                | '\u{60}'..='\u{7e}' => {
                    return self.change_state(State::Literal, Action::EscDispatch, c);
                }
                '\u{5b}' => {
                    return self.change_state(State::CtrlStart, Action::None, c);
                }
                '\u{5d}' => {
                    return self.change_state(State::OsCmd, Action::None, c);
                }
                '\u{50}' => {
                    return self.change_state(State::DevCtrlStart, Action::None, c);
                }
                '\u{58}' | '\u{5e}' | '\u{5f}' => {
                    return self.change_state(State::IgnoreUntilSt, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::EscapeIntermediate => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' => {
                    return self.action(Action::Execute, c);
                }
                '\u{7f}' => {
                    return; // Ignored
                }
                '\u{20}'..='\u{2f}' => {
                    return self.action(Action::Collect, c);
                }
                '\u{30}'..='\u{7e}' => {
                    return self.change_state(State::Literal, Action::EscDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlStart => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' => {
                    return self.action(Action::Execute, c);
                }
                '\u{7f}' => {
                    return; // Ignored
                }
                '\u{20}'..='\u{2f}' => {
                    return self.change_state(State::CtrlIntermediate, Action::Collect, c);
                }
                '\u{3a}' => {
                    return self.change_state(State::CtrlMalformed, Action::None, c);
                }
                '\u{30}'..='\u{39}' | '\u{3b}' => {
                    return self.change_state(State::CtrlParam, Action::Param, c);
                }
                '\u{3c}'..='\u{3f}' => {
                    return self.change_state(State::CtrlParam, Action::Collect, c);
                }
                '\u{40}'..='\u{7e}' => {
                    return self.change_state(State::Literal, Action::CsiDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlParam => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' => {
                    return self.action(Action::Execute, c);
                }
                '\u{30}'..='\u{39}' | '\u{3b}' => {
                    return self.action(Action::Param, c);
                }
                '\u{7f}' => {
                    return; // Ignored
                }
                '\u{3a}' | '\u{3c}'..='\u{3f}' => {
                    return self.change_state(State::CtrlMalformed, Action::None, c);
                }
                '\u{20}'..='\u{2f}' => {
                    return self.change_state(State::CtrlIntermediate, Action::Collect, c);
                }
                '\u{40}'..='\u{7e}' => {
                    return self.change_state(State::Literal, Action::CsiDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlIntermediate => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' => {
                    return self.action(Action::Execute, c);
                }
                '\u{20}'..='\u{2f}' => {
                    return self.action(Action::Collect, c);
                }
                '\u{7f}' => {
                    return; // Ignored
                }
                '\u{3a}' | '\u{3c}'..='\u{3f}' => {
                    return self.change_state(State::CtrlMalformed, Action::None, c);
                }
                '\u{40}'..='\u{7e}' => {
                    return self.change_state(State::Literal, Action::CsiDispatch, c);
                }
                _ => return self.error(c),
            },
            State::CtrlMalformed => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' => {
                    return self.action(Action::Execute, c);
                }
                '\u{20}'..='\u{3f}' | '\u{7f}' => {
                    return; // Ignored
                }
                '\u{40}'..='\u{7e}' => {
                    return self.change_state(State::Literal, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlStart => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' | '\u{7f}' => {
                    return; // Ignored
                }
                '\u{3a}' => {
                    return self.change_state(State::DevCtrlMalformed, Action::None, c);
                }
                '\u{20}'..='\u{2f}' => {
                    return self.change_state(State::DevCtrlIntermediate, Action::Collect, c);
                }
                '\u{30}'..='\u{39}' | '\u{3b}' => {
                    return self.change_state(State::DevCtrlParam, Action::Param, c);
                }
                '\u{3c}'..='\u{3f}' => {
                    return self.change_state(State::DevCtrlParam, Action::Collect, c);
                }
                '\u{40}'..='\u{7e}' => {
                    return self.change_state(State::DevCtrlPassthru, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlParam => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' | '\u{7f}' => {
                    return; // Ignored
                }
                '\u{30}'..='\u{39}' | '\u{3b}' => {
                    return self.action(Action::Param, c);
                }
                '\u{3a}' | '\u{3c}'..='\u{3f}' => {
                    return self.change_state(State::DevCtrlMalformed, Action::None, c);
                }
                '\u{20}'..='\u{2f}' => {
                    return self.change_state(State::DevCtrlIntermediate, Action::Collect, c);
                }
                '\u{40}'..='\u{7e}' => {
                    return self.change_state(State::DevCtrlPassthru, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlIntermediate => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' | '\u{7f}' => {
                    return; // Ignored
                }
                '\u{20}'..='\u{2f}' => {
                    return self.action(Action::Collect, c);
                }
                '\u{30}'..='\u{3f}' => {
                    return self.change_state(State::DevCtrlMalformed, Action::None, c);
                }
                '\u{40}'..='\u{7e}' => {
                    return self.change_state(State::DevCtrlPassthru, Action::None, c);
                }
                _ => return self.error(c),
            },
            State::DevCtrlPassthru => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' | '\u{20}'..='\u{7e}' => {
                    return self.action(Action::Put, c);
                }
                '\u{7f}' => {
                    return; // Ignored
                }
                _ => return self.error(c),
            },
            State::DevCtrlMalformed => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' | '\u{20}'..='\u{7f}' => {
                    return; // Ignored
                }
                _ => return self.error(c),
            },
            State::OsCmd => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' => {
                    return; // Ignored
                }
                '\u{20}'..='\u{7f}' => {
                    return self.action(Action::OscPut, c);
                }
                _ => return self.error(c),
            },
            State::IgnoreUntilSt => match c {
                '\u{00}'..='\u{17}' | '\u{19}' | '\u{1c}'..='\u{1f}' | '\u{20}'..='\u{7f}' => {
                    return; // Ignored
                }
                _ => return self.error(c),
            },
        }
    }

    fn action(&mut self, action: Action, c: char) {
        match action {
            Action::Print => self.handler.print(c),
            Action::Execute => self.handler.execute_ctrl(c),
            Action::Hook => self.handler.dcs_start(c, &self.params, &self.intermediates),
            Action::Put => self.handler.dcs_char(c),
            Action::OscStart => self.handler.osc_start(c),
            Action::OscPut => self.handler.osc_char(c),
            Action::OscEnd => self.handler.osc_end(c),
            Action::Unhook => self.handler.dcs_end(c),
            Action::CsiDispatch => {
                self.handler
                    .dispatch_csi(c, &self.params, &self.intermediates);
            }
            Action::EscDispatch => self.handler.dispatch_esc(c, &self.intermediates),
            Action::None => {}
            Action::Collect => self.intermediates.push(c),
            Action::Param => {
                self.params.push_csi_char(c);
            }
            Action::Clear => {
                self.intermediates.clear();
                self.params.clear();
            }
        }
    }

    fn change_state(&mut self, state: State, transition: Action, c: char) {
        self.state_exit_actions(self.state, c);
        self.state = state;
        self.action(transition, c);
        self.state_entry_actions(state, c);
    }

    fn state_entry_actions(&mut self, state: State, c: char) {
        match state {
            State::Escape => self.action(Action::Clear, c),
            State::CtrlStart => self.action(Action::Clear, c),
            State::DevCtrlStart => self.action(Action::Clear, c),
            State::OsCmd => self.action(Action::OscStart, c),
            State::DevCtrlPassthru => self.action(Action::Hook, c),
            _ => {}
        }
    }

    fn state_exit_actions(&mut self, state: State, c: char) {
        match state {
            State::OsCmd => self.action(Action::OscEnd, c),
            State::DevCtrlPassthru => self.action(Action::Unhook, c),
            _ => {}
        }
    }

    fn error(&mut self, c: char) {
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
    len: usize,
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
        ret.len = from.len();
        (&mut ret.buf[..from.len()]).copy_from_slice(from);
        ret
    }

    /// Attempts to push a new value.
    ///
    /// A [`VtParams`] has a capacity of 16 items, and so any pushes after
    /// that capacity has been reached are silently ignored.
    pub fn push(&mut self, v: u16) {
        if self.len == self.buf.len() {
            return; // pushes beyond capacity are silently ignored
        }
        self.buf[self.len] = v;
        self.len += 1;
    }

    fn push_csi_char(&mut self, c: char) {
        if c == ';' {
            // Argument separator, so we start a new param.
            self.push(0);
        } else {
            // The character must be a digit, then
            if self.len == 0 {
                self.push(0); // start our first param
            }
            let current = &mut self.buf[self.len - 1];
            let digit = (c as u16) - ('0' as u16);
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
        &self.buf[..self.len]
    }

    /// Returns the current number of parameters.
    #[inline(always)]
    pub const fn len(&self) -> usize {
        self.len
    }
}

impl core::fmt::Debug for VtParams {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("VtParams")
            .field(&&self.buf[..self.len])
            .finish()
    }
}

/// Zero or more intermediate characters that appeared as part of an
/// escape sequence.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VtIntermediates {
    buf: [char; 2],
    len: u8, // greater than length of buf means overrun
}

impl VtIntermediates {
    const OVERRUN_LEN: usize = 3;

    /// Constructs a new zero-length [`VtIntermediates`].
    pub const fn new() -> Self {
        Self {
            buf: ['\0'; 2],
            len: 0,
        }
    }

    /// Constructs a new [`VtIntermediates`] containing the values in the given slice.
    ///
    /// A `VtIntermediates` has a maximum capacity of two items, so this will panic if
    /// the given slice has length three or greater.
    pub fn from_slice(from: &[char]) -> Self {
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
    pub fn push(&mut self, c: char) {
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

    /// Returns the intermediate characters as a slice of [`char`] values.
    pub fn chars(&self) -> &[char] {
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
