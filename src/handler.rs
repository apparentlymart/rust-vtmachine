use crate::{VtIntermediates, VtParams};
use u8char::u8char;

/// Trait for implementations that can process events from a [`crate::VtMachine`].
///
/// All of the provided method implementations perform no action at all.
pub trait VtHandler {
    /// Emits a character at the current cursor position.
    ///
    /// For a terminal implementation that wants to combine individual scalar values
    /// into [grapheme clusters](https://www.unicode.org/reports/tr29/#Grapheme_Cluster_Boundaries),
    /// that must be handled by the implementation itself.
    #[inline(always)]
    fn print(&mut self, c: u8char) {
        let _ = c;
        // Silently ignored by default.
    }

    /// Executes an individual C0 or C1 control character, such as newline, carriage return,
    /// horizontal tab, etc.
    #[inline(always)]
    fn execute_ctrl(&mut self, c: u8) {
        let _ = c;
        // Silently ignores individual control characters by default.
    }

    /// Executes a control function that began with a Control Sequence Introducer (CSI).
    ///
    /// `cmd` is the final character that decides which function to execute. `params` and
    /// `intermediates` provide the numeric parameters and intermediate characters that
    /// appeared between the introducer and the final character.
    ///
    /// It's up to the implementation to assign meaning to `cmd` and the other arguments.
    #[inline(always)]
    fn dispatch_csi(&mut self, cmd: u8, params: &VtParams, intermediates: &VtIntermediates) {
        let _ = (cmd, params, intermediates);
        // Silently ignored by default.
    }

    /// Executes an escape sequence that did not begin with a Control Sequence Introducer (CSI).
    ///
    /// `cmd` is the character that decides which function to execute. `intermediates` provides
    /// any intermediate characters that appeared between the initial ESC and the final
    /// character.
    #[inline(always)]
    fn dispatch_esc(&mut self, cmd: u8, intermediates: &VtIntermediates) {
        let _ = (cmd, intermediates);
        // Silently ignored by default.
    }

    /// Handles an unexpected character.
    ///
    /// [`crate::VtMachine`] reports this when it encounters a character that isn't
    /// valid to appear at the current state. It's up to the implementation how to handle
    /// such characters, if at all.
    #[inline(always)]
    fn error(&mut self, c: u8char) {
        let _ = c;
        // Silently ignores errors by default.
    }

    /// Handles the beginning of a device control string.
    ///
    /// A typical implementation will decide on a handler based on the arguments and then
    /// prepare to recieve zero or more calls to [`VtHandler::dcs_char`] followed by
    /// one call to [`VtHandler::dcs_end`].
    #[inline(always)]
    fn dcs_start(&mut self, cmd: u8, params: &VtParams, intermediates: &VtIntermediates) {
        let _ = (cmd, params, intermediates);
        // Silently ignored by default.
    }

    /// Handles a character appearing as part of a device control string.
    ///
    /// This is only called when there has been a previous [`VtHandler::dcs_start`] that
    /// has not yet been closed by a [`VtHandler::dcs_end`].
    #[inline(always)]
    fn dcs_char(&mut self, c: u8char) {
        let _ = c;
        // Silently ignored by default.
    }

    /// Handles the end of a device control string.
    ///
    /// This only appears after an earlier call to [`VtHandler::dcs_start`].
    #[inline(always)]
    fn dcs_end(&mut self, c: u8) {
        let _ = c;
        // Silently ignored by default.
    }

    /// Handles the beginning of an operating system command.
    ///
    /// This will be followed by zero or more [`VtHandler::osc_char`] and then one
    /// [`VtHandler::osc_end`].
    #[inline(always)]
    fn osc_start(&mut self, c: u8) {
        let _ = c;
        // Silently ignored by default.
    }

    /// Handles a character appearing as part of an operating system command.
    ///
    /// This is only called when there has been a previous [`VtHandler::osc_start`] that
    /// has not yet been closed by a [`VtHandler::osc_end`].
    #[inline(always)]
    fn osc_char(&mut self, c: u8char) {
        let _ = c;
        // Silently ignored by default.
    }

    /// Handles the end of an operating system command.
    ///
    /// This only appears after an earlier call to [`VtHandler::osc_start`].
    #[inline(always)]
    fn osc_end(&mut self, c: u8) {
        let _ = c;
        // Silently ignored by default.
    }
}

/// Represents terminal events delivered to a callback through `vt_handler_fn`.
///
/// Each variant corresponds to a method of [`VtHandler`]. Unlike when implementing
/// `VtHandler` directly, all provided [`VtParams`] and [`VtIntermediates`] values
/// are owned and independent of the lifetime of any [`crate::VtMachine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtEvent {
    Print(u8char),
    ExecuteCtrl(u8),
    DispatchCsi {
        cmd: u8,
        params: VtParams,
        intermediates: VtIntermediates,
    },
    DispatchEsc {
        cmd: u8,
        intermediates: VtIntermediates,
    },
    DcsStart {
        cmd: u8,
        params: VtParams,
        intermediates: VtIntermediates,
    },
    DcsChar(u8char),
    DcsEnd(u8),
    OscStart(u8),
    OscChar(u8char),
    OscEnd(u8),
    Error(u8char),
}

/// Returns a [`VtHandler`] that calls the given function for each
/// event produced by an associated [`crate::VtMachine`].
///
/// This can potentially be a convenient way to implement `VtHandler`,
/// but comes at the cost of forcing copies of any [`VtParams`] or
/// [`VtIntermediates`] values in the emitted events, whereas
/// directly implementing `VtHandler` provides direct references to
/// the state machine's data.
pub fn vt_handler_fn(f: impl FnMut(VtEvent)) -> impl VtHandler {
    VtHandlerFn { f }
}

struct VtHandlerFn<F> {
    f: F,
}

impl<F: FnMut(VtEvent)> VtHandler for VtHandlerFn<F> {
    #[inline(always)]
    fn print(&mut self, c: u8char) {
        (self.f)(VtEvent::Print(c));
    }

    #[inline(always)]
    fn execute_ctrl(&mut self, c: u8) {
        (self.f)(VtEvent::ExecuteCtrl(c));
    }

    #[inline(always)]
    fn dispatch_csi(&mut self, cmd: u8, params: &VtParams, intermediates: &VtIntermediates) {
        (self.f)(VtEvent::DispatchCsi {
            cmd,
            params: *params,
            intermediates: *intermediates,
        });
    }

    #[inline(always)]
    fn error(&mut self, c: u8char) {
        (self.f)(VtEvent::Error(c));
    }

    #[inline(always)]
    fn dispatch_esc(&mut self, cmd: u8, intermediates: &VtIntermediates) {
        (self.f)(VtEvent::DispatchEsc {
            cmd,
            intermediates: *intermediates,
        });
    }

    #[inline(always)]
    fn dcs_start(&mut self, cmd: u8, params: &VtParams, intermediates: &VtIntermediates) {
        (self.f)(VtEvent::DcsStart {
            cmd,
            params: *params,
            intermediates: *intermediates,
        });
    }

    #[inline(always)]
    fn dcs_char(&mut self, c: u8char) {
        (self.f)(VtEvent::DcsChar(c));
    }

    #[inline(always)]
    fn dcs_end(&mut self, c: u8) {
        (self.f)(VtEvent::DcsEnd(c));
    }

    #[inline(always)]
    fn osc_start(&mut self, c: u8) {
        (self.f)(VtEvent::OscStart(c));
    }

    #[inline(always)]
    fn osc_char(&mut self, c: u8char) {
        (self.f)(VtEvent::OscChar(c));
    }

    #[inline(always)]
    fn osc_end(&mut self, c: u8) {
        (self.f)(VtEvent::OscEnd(c));
    }
}
