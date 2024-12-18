use super::*;

extern crate std;
use std::vec::Vec;

#[test]
fn literal() {
    let mut m = testing_machine();
    m.write("hello world\r\nboop");
    let log = m.handler().log();
    assert_eq!(
        log,
        &[
            VtEvent::Print('h'),
            VtEvent::Print('e'),
            VtEvent::Print('l'),
            VtEvent::Print('l'),
            VtEvent::Print('o'),
            VtEvent::Print(' '),
            VtEvent::Print('w'),
            VtEvent::Print('o'),
            VtEvent::Print('r'),
            VtEvent::Print('l'),
            VtEvent::Print('d'),
            VtEvent::ExecuteCtrl('\r'),
            VtEvent::ExecuteCtrl('\n'),
            VtEvent::Print('b'),
            VtEvent::Print('o'),
            VtEvent::Print('o'),
            VtEvent::Print('p'),
        ]
    );
}

#[test]
fn format_csi() {
    let mut m = testing_machine();
    m.write("plain\x1b[1mbold\x1b[2;3pmore");
    let log = m.handler().log();
    assert_eq!(
        log,
        &[
            VtEvent::Print('p'),
            VtEvent::Print('l'),
            VtEvent::Print('a'),
            VtEvent::Print('i'),
            VtEvent::Print('n'),
            VtEvent::DispatchCsi {
                cmd: 'm',
                params: VtParams::from_slice(&[1]),
                intermediates: VtIntermediates::new(),
            },
            VtEvent::Print('b'),
            VtEvent::Print('o'),
            VtEvent::Print('l'),
            VtEvent::Print('d'),
            VtEvent::DispatchCsi {
                cmd: 'p',
                params: VtParams::from_slice(&[2, 3]),
                intermediates: VtIntermediates::new(),
            },
            VtEvent::Print('m'),
            VtEvent::Print('o'),
            VtEvent::Print('r'),
            VtEvent::Print('e'),
        ]
    );
}

fn testing_machine() -> VtMachine<LogHandler> {
    VtMachine::new(LogHandler::new())
}

struct LogHandler {
    log: Vec<VtEvent>,
}

impl LogHandler {
    pub fn new() -> Self {
        Self { log: Vec::new() }
    }

    pub fn log(&self) -> &[VtEvent] {
        &self.log
    }
}

impl VtHandler for LogHandler {
    #[inline(always)]
    fn print(&mut self, c: char) {
        self.log.push(VtEvent::Print(c));
    }

    #[inline(always)]
    fn execute_ctrl(&mut self, c: char) {
        self.log.push(VtEvent::ExecuteCtrl(c));
    }

    #[inline(always)]
    fn dispatch_csi(&mut self, cmd: char, params: &VtParams, intermediates: &VtIntermediates) {
        self.log.push(VtEvent::DispatchCsi {
            cmd,
            params: *params,
            intermediates: *intermediates,
        });
    }

    #[inline(always)]
    fn error(&mut self, c: char) {
        self.log.push(VtEvent::Error(c));
    }
}
