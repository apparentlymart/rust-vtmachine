use super::*;

extern crate std;
use std::vec::Vec;
use u8char::u8char;

macro_rules! print_event {
    ($c:literal) => {
        VtEvent::Print(u8char::from_char($c))
    };
}

#[test]
fn literal() {
    let mut m = testing_machine();
    m.write("hello world\r\nboop");
    let log = m.handler().log();
    assert_eq!(
        log,
        &[
            print_event!('h'),
            print_event!('e'),
            print_event!('l'),
            print_event!('l'),
            print_event!('o'),
            print_event!(' '),
            print_event!('w'),
            print_event!('o'),
            print_event!('r'),
            print_event!('l'),
            print_event!('d'),
            VtEvent::PrintEnd,
            VtEvent::ExecuteCtrl('\r' as u8),
            VtEvent::ExecuteCtrl('\n' as u8),
            print_event!('b'),
            print_event!('o'),
            print_event!('o'),
            print_event!('p'),
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
            print_event!('p'),
            print_event!('l'),
            print_event!('a'),
            print_event!('i'),
            print_event!('n'),
            VtEvent::PrintEnd,
            VtEvent::DispatchCsi {
                cmd: 'm' as u8,
                params: VtParams::from_slice(&[1]),
                intermediates: VtIntermediates::new(),
            },
            print_event!('b'),
            print_event!('o'),
            print_event!('l'),
            print_event!('d'),
            VtEvent::PrintEnd,
            VtEvent::DispatchCsi {
                cmd: 'p' as u8,
                params: VtParams::from_slice(&[2, 3]),
                intermediates: VtIntermediates::new(),
            },
            print_event!('m'),
            print_event!('o'),
            print_event!('r'),
            print_event!('e'),
        ]
    );
}

#[test]
fn through_u8char_stream() {
    // The docs for `VtMachine::write` recommend using
    // `u8char::stream::U8CharStream` to consume a stream of UTF-8 bytes
    // like what might arrive through a pseudoterminal device, so this
    // is a simple test making sure that idea keeps working.
    let mut m = testing_machine();
    let mut stream = ::u8char::stream::U8CharStream::new();
    for c in stream.more(b"a\x1b[1m\xe2\x9d\x9e\x1b[0m\x9dc\xe2") {
        m.write_u8char(c);
    }
    for c in stream.end() {
        m.write_u8char(c);
    }
    m.write_end();
    let log = m.handler().log();
    assert_eq!(
        log,
        &[
            print_event!('a'),
            VtEvent::PrintEnd,
            VtEvent::DispatchCsi {
                cmd: 'm' as u8,
                params: VtParams::from_slice(&[1]),
                intermediates: VtIntermediates::new(),
            },
            print_event!('❞'),
            VtEvent::PrintEnd,
            VtEvent::DispatchCsi {
                cmd: 'm' as u8,
                params: VtParams::from_slice(&[0]),
                intermediates: VtIntermediates::new(),
            },
            print_event!('\u{FFFD}'),
            print_event!('c'),
            print_event!('\u{FFFD}'),
            VtEvent::PrintEnd,
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
    fn print(&mut self, c: u8char) {
        self.log.push(VtEvent::Print(c));
    }

    #[inline(always)]
    fn print_end(&mut self) {
        self.log.push(VtEvent::PrintEnd);
    }

    #[inline(always)]
    fn execute_ctrl(&mut self, c: u8) {
        self.log.push(VtEvent::ExecuteCtrl(c));
    }

    #[inline(always)]
    fn dispatch_csi(&mut self, cmd: u8, params: &VtParams, intermediates: &VtIntermediates) {
        self.log.push(VtEvent::DispatchCsi {
            cmd,
            params: *params,
            intermediates: *intermediates,
        });
    }

    #[inline(always)]
    fn error(&mut self, c: u8char) {
        self.log.push(VtEvent::Error(c));
    }
}
