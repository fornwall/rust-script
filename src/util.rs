/*!
This module just contains other random implementation stuff.
*/
use std::io;
use std::io::prelude::*;

/**
A `Write` filter that turns everything into lowercase hex text.
*/
pub struct Hexify<W>(pub W) where W: Write;

impl<W> Write for Hexify<W>
where W: Write {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match buf.into_iter().next() {
            Some(b) => {
                try!(write!(self.0, "{:x}", b));
                Ok(1)
            },
            None => Ok(0)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
