use std::io::{stdin, Read};

use vtmachine::VtMachine;

fn main() -> Result<(), std::io::Error> {
    let mut machine = VtMachine::new();
    let mut char_stream = u8char::stream::U8CharStream::new();

    let mut r = stdin();
    let mut buf = [0_u8; 64];
    loop {
        // VtMachine wants u8char as input, so we need to interpret stdin as
        // UTF-8 first before we can feed chunks to the state machine.
        let read_len = r.read(&mut buf[..])?;
        if read_len == 0 {
            return Ok(());
        }
        let buf = &buf[..read_len];
        for c in char_stream.more(buf) {
            for event in machine.write_u8char(c) {
                println!("{event:?}");
            }
        }
    }
}
