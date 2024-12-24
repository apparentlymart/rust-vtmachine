use core::str;
use std::io::{stdin, Read};

use vtmachine::{VtHandler, VtMachine};

fn main() -> Result<(), std::io::Error> {
    let mut machine = VtMachine::new(Handler);

    let mut r = stdin();
    let mut buf = [0_u8; 64];
    let mut read_start = 0_usize;
    loop {
        // VtMachine wants str or char as input, so we need to interpret stdin as
        // UTF-8 first before we can feed chunks to the state machine.
        let read_len = r.read(&mut buf[read_start..])? + read_start;
        if read_len == 0 {
            return Ok(());
        }
        let mut remain = &buf[..read_len];
        read_start = 0;

        'chunk: while !remain.is_empty() {
            match str::from_utf8(remain) {
                Ok(s) => {
                    machine.write(s);
                    break 'chunk;
                }
                Err(e) => {
                    let (valid, leftover) = remain.split_at(e.valid_up_to());
                    let valid = unsafe {
                        // Safety: the original str::from_utf8 promised up that this
                        // much of the input was valid utf8.
                        str::from_utf8_unchecked(valid)
                    };
                    machine.write(valid);

                    match e.error_len() {
                        Some(skip) => {
                            // We've encountered something invalid that is `skip`
                            // bytes long, so we'll emit the "unicode replacement character"
                            // and then continue afterwards.
                            machine.write_char('\u{FFFD}');
                            remain = &leftover[skip..];
                        }
                        None => {
                            // We seem to have only part of a UTF-8 sequence in
                            // leftover, so we need to save it for our next
                            // iteration. By definition there can't be more than
                            // three bytes left, or we'd be in the Some arm instead.
                            read_start = leftover.len();
                            let mut keep = [0_u8; 3];
                            (&mut keep[..read_start]).copy_from_slice(leftover);
                            (&mut buf[..read_start]).copy_from_slice(&keep[..read_start]);
                            break 'chunk;
                        }
                    }
                }
            }
        }
    }
}

struct Handler;

impl VtHandler for Handler {
    fn print(&mut self, c: char) {
        println!("print({c:?})");
    }

    fn execute_ctrl(&mut self, c: char) {
        println!("execute_ctrl({c:?})");
    }

    fn dispatch_csi(
        &mut self,
        cmd: char,
        params: &vtmachine::VtParams,
        intermediates: &vtmachine::VtIntermediates,
    ) {
        println!("dispatch_csi({cmd:?}, {params:?}, {intermediates:?})");
    }

    fn dispatch_esc(&mut self, cmd: char, intermediates: &vtmachine::VtIntermediates) {
        println!("dispatch_esc({cmd:?}, {intermediates:?})");
    }

    fn error(&mut self, c: char) {
        println!("error({c:?})");
    }

    fn dcs_start(
        &mut self,
        cmd: char,
        params: &vtmachine::VtParams,
        intermediates: &vtmachine::VtIntermediates,
    ) {
        println!("dcs_start({cmd:?}, {params:?}, {intermediates:?})");
    }

    fn dcs_char(&mut self, c: char) {
        println!("dcs_char({c:?})");
    }

    fn dcs_end(&mut self, c: char) {
        println!("dcs_end({c:?})");
    }

    fn osc_start(&mut self, c: char) {
        println!("osc_start({c:?})");
    }

    fn osc_char(&mut self, c: char) {
        println!("osc_char({c:?})");
    }

    fn osc_end(&mut self, c: char) {
        println!("osc_end({c:?})");
    }
}
