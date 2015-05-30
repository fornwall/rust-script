/*!
This module just contains other random implementation stuff.
*/
use std::error::Error;
use std::io;
use std::io::prelude::*;
use std::marker::PhantomData;

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

/**
Used to defer a closure until the value is dropped.

The closure *must* return a `Result<(), _>`, as a reminder to *not* panic; doing so will abort your whole program if it happens during another panic.  If the closure returns an `Err`, then it is logged as an `error`.

A `Defer` can also be "disarmed", preventing the closure from running at all.
*/
#[must_use]
pub struct Defer<'a, F, E>(Option<F>, PhantomData<&'a F>)
where F: 'a + FnOnce() -> Result<(), E>,
    E: Error;

impl<'a, F, E> Defer<'a, F, E>
where F: 'a + FnOnce() -> Result<(), E>,
    E: Error
{
    /**
    Create a new `Defer` with the given closure.
    */
    pub fn defer(f: F) -> Defer<'a, F, E> {
        Defer(Some(f), PhantomData)
    }

    /**
    Consume this `Defer` *without* invoking the closure.
    */
    pub fn disarm(mut self) {
        self.0 = None;
        drop(self);
    }
}

impl<'a, F, E> ::std::ops::Drop for Defer<'a, F, E>
where
    F: 'a + FnOnce() -> Result<(), E>,
    E: Error
{
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            if let Err(err) = f() {
                error!("deferred function failed: {}", err);
            }
        }
    }
}
