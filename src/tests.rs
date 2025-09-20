use super::*;

extern crate std;
use pretty_assertions::assert_eq;
use std::string::String;
use std::vec::Vec;
use u8char::u8char;

macro_rules! print_event {
    ($c:literal) => {
        VtEvent::Print(u8char::from_char($c))
    };
}

#[test]
fn literal() {
    let mut m = VtMachine::new();
    let got = collect_events(&mut m, "hello world\r\nboop");
    let want = want_events(&[
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
        VtEvent::ExecuteCtrl(b'\r'),
        VtEvent::ExecuteCtrl(b'\n'),
        print_event!('b'),
        print_event!('o'),
        print_event!('o'),
        print_event!('p'),
        VtEvent::PrintEnd,
    ]);
    assert_eq!(got, want);
}

#[test]
fn format_csi() {
    let mut m = VtMachine::new();
    let got = collect_events(&mut m, "plain\x1b[1mbold\x1b[2;3pmore");
    let want = want_events(&[
        print_event!('p'),
        print_event!('l'),
        print_event!('a'),
        print_event!('i'),
        print_event!('n'),
        VtEvent::PrintEnd,
        VtEvent::DispatchCsi {
            cmd: 'm' as u8,
            params: &[1],
            intermediates: &[],
        },
        print_event!('b'),
        print_event!('o'),
        print_event!('l'),
        print_event!('d'),
        VtEvent::PrintEnd,
        VtEvent::DispatchCsi {
            cmd: 'p' as u8,
            params: &[2, 3],
            intermediates: &[],
        },
        print_event!('m'),
        print_event!('o'),
        print_event!('r'),
        print_event!('e'),
        VtEvent::PrintEnd,
    ]);
    assert_eq!(got, want);
}

#[test]
fn through_u8char_stream() {
    use std::format;

    // The docs for `VtMachine::write` recommend using
    // `u8char::stream::U8CharStream` to consume a stream of UTF-8 bytes
    // like what might arrive through a pseudoterminal device, so this
    // is a simple test making sure that idea keeps working.
    let mut m = VtMachine::new();
    let mut stream = ::u8char::stream::U8CharStream::new();
    let mut got: Vec<String> = Vec::new();

    for c in stream.more(b"a\x1b[1m\xe2\x9d\x9e\x1b[0m\x9dc\xe2") {
        for event in m.write_u8char(c) {
            got.push(format!("{event:?}"));
        }
    }
    for c in stream.end() {
        for event in m.write_u8char(c) {
            got.push(format!("{event:?}"));
        }
    }
    for event in m.write_end() {
        got.push(format!("{event:?}"));
    }

    let want = want_events(&[
        print_event!('a'),
        VtEvent::PrintEnd,
        VtEvent::DispatchCsi {
            cmd: 'm' as u8,
            params: &[1],
            intermediates: &[],
        },
        print_event!('❞'),
        VtEvent::PrintEnd,
        VtEvent::DispatchCsi {
            cmd: 'm' as u8,
            params: &[0],
            intermediates: &[],
        },
        print_event!('\u{FFFD}'),
        print_event!('c'),
        print_event!('\u{FFFD}'),
        VtEvent::PrintEnd,
    ]);
    assert_eq!(got, want);
}

fn collect_events(machine: &mut VtMachine, input: &str) -> Vec<String> {
    use ::u8char::AsU8Chars;
    use std::format;

    let mut ret: Vec<String> = Vec::new();
    for c in input.u8chars() {
        for event in machine.write_u8char(c) {
            ret.push(format!("{event:?}"));
        }
    }
    for event in machine.write_end() {
        ret.push(format!("{event:?}"));
    }
    ret
}

fn want_events(events: &[VtEvent]) -> Vec<String> {
    use std::format;

    let mut ret: Vec<String> = Vec::with_capacity(events.len());
    for event in events {
        ret.push(format!("{event:?}"));
    }
    ret
}
