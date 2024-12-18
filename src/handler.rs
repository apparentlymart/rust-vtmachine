use crate::{VtIntermediates, VtParams};

pub trait VtHandler {
    fn print(&mut self, c: char);

    #[inline(always)]
    fn execute_ctrl(&mut self, c: char) {
        let _ = c;
        // Silently ignores individual control characters by default.
    }

    #[inline(always)]
    fn dispatch_csi(&mut self, cmd: char, params: &VtParams, intermediates: &VtIntermediates) {
        let _ = (cmd, params, intermediates);
        // Silently ignored by default.
    }

    #[inline(always)]
    fn dispatch_esc(&mut self, cmd: char, intermediates: &VtIntermediates) {
        let _ = (cmd, intermediates);
        // Silently ignored by default.
    }

    #[inline(always)]
    fn error(&mut self, c: char) {
        let _ = c;
        // Silently ignores errors by default.
    }

    #[inline(always)]
    fn dcs_start(&mut self, cmd: char, params: &VtParams, intermediates: &VtIntermediates) {
        let _ = (cmd, params, intermediates);
        // Silently ignored by default.
    }

    #[inline(always)]
    fn dcs_char(&mut self, c: char) {
        let _ = c;
        // Silently ignored by default.
    }

    #[inline(always)]
    fn dcs_end(&mut self, c: char) {
        let _ = c;
        // Silently ignored by default.
    }

    #[inline(always)]
    fn osc_start(&mut self, c: char) {
        let _ = c;
        // Silently ignored by default.
    }

    #[inline(always)]
    fn osc_char(&mut self, c: char) {
        let _ = c;
        // Silently ignored by default.
    }

    #[inline(always)]
    fn osc_end(&mut self, c: char) {
        let _ = c;
        // Silently ignored by default.
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VtEvent {
    Print(char),
    ExecuteCtrl(char),
    DispatchCsi {
        cmd: char,
        params: VtParams,
        intermediates: VtIntermediates,
    },
    Error(char),
}

/// Returns a [`VtHandler`] that calls the given function for each
/// event produced by an associated [`crate::VtMachine`].
pub fn vt_handler_fn(f: impl FnMut(VtEvent)) -> impl VtHandler {
    VtHandlerFn { f }
}

struct VtHandlerFn<F> {
    f: F,
}

impl<F: FnMut(VtEvent)> VtHandler for VtHandlerFn<F> {
    #[inline(always)]
    fn print(&mut self, c: char) {
        (self.f)(VtEvent::Print(c));
    }

    #[inline(always)]
    fn execute_ctrl(&mut self, c: char) {
        (self.f)(VtEvent::ExecuteCtrl(c));
    }

    fn dispatch_csi(&mut self, cmd: char, params: &VtParams, intermediates: &VtIntermediates) {
        (self.f)(VtEvent::DispatchCsi {
            cmd,
            params: *params,
            intermediates: *intermediates,
        });
    }

    #[inline(always)]
    fn error(&mut self, c: char) {
        (self.f)(VtEvent::Error(c));
    }
}
