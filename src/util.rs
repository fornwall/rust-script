/*
Copyright ⓒ 2015-2017 cargo-script contributors.

Licensed under the MIT license (see LICENSE or <http://opensource.org
/licenses/MIT>) or the Apache License, Version 2.0 (see LICENSE of
<http://www.apache.org/licenses/LICENSE-2.0>), at your option. All
files in the project carrying such notice may not be copied, modified,
or distributed except according to those terms.
*/
/*!
This module just contains other random implementation stuff.
*/
use std::error::Error;
use std::marker::PhantomData;

/**
A really, really hacky way of avoiding a variable binding.
*/
pub trait ChainMap: Sized {
    fn chain_map<F>(self, f: F) -> Self
    where F: FnOnce(Self) -> Self {
        f(self)
    }
}

impl<T> ChainMap for T {}

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

#[cfg(feature="suppress-cargo-output")]
pub use self::suppress_child_output::{ChildToken, suppress_child_output};

#[cfg(feature="suppress-cargo-output")]
mod suppress_child_output {
    use std::io;
    use std::process::{self, Command};
    use std::thread;
    use std::time::Duration;
    use crossbeam_channel;
    use crate::error::Result;

    /**
    Suppresses the stderr output of a child process, unless:

    - the process takes longer than `timeout` to complete, or
    - the process exits and signals a failure.

    In either of those cases, the existing output is flushes to the current process' stderr, and all further output from the child is passed through.

    In other words: if the child successfully completes quickly, it's stderr output is suppressed.  Otherwise, it's let through.
    */
    pub fn suppress_child_output(cmd: &mut Command, timeout: Duration) -> Result<ChildToken> {
        cmd.stderr(process::Stdio::piped());

        let mut child = cmd.spawn()?;
        let stderr = child.stderr.take().expect("no stderr pipe found");

        let timeout_chan = crossbeam_channel::after(timeout);
        let (done_sig, done_gate) = crossbeam_channel::bounded(0);

        let _ = thread::spawn(move || {
            let show_stderr;
            let mut recv_done = false;
            select! {
                recv(timeout_chan) -> _ => {
                    show_stderr = true;
                },
                recv(done_gate) -> success => {
                    show_stderr = !success.unwrap_or(true);
                    recv_done = true;
                },
            }
            if show_stderr {
                let mut stderr = stderr;
                io::copy(&mut stderr, &mut io::stderr())
                    .expect("could not copy child stderr");
            }
            if !recv_done {
                done_gate.recv().unwrap();
            }
        });

        Ok(ChildToken {
            child: child,
            done_sig: Some(done_sig),
            // stderr_join: stderr_join,
        })
    }

    pub struct ChildToken {
        child: process::Child,
        done_sig: Option<crossbeam_channel::Sender<bool>>,
        // stderr_join: Option<thread::JoinHandle<()>>,
    }

    impl ChildToken {
        pub fn status(&mut self) -> io::Result<process::ExitStatus> {
            let st = match self.child.wait() {
                Ok(r) => r,
                Err(e) => {
                    if let Some(done_sig) = self.done_sig.take() {
                        done_sig.send(false).unwrap();
                    }
                    return Err(e.into());
                }
            };
            if let Some(done_sig) = self.done_sig.take() {
                done_sig.send(st.success()).unwrap();
            }
            // if let Some(stderr_join) = self.stderr_join.take() {
            //     stderr_join.join()
            //         .expect("child stderr thread failed");
            // }
            Ok(st)
        }
    }
}

/**
This *used* to be in the stdlib, until it was deprecated and removed after being replaced by better, pattern-based methods.

This might cause you to wonder why I'm not using these methods.  That is because *they don't exist*.

Can you tell I'm *really annoyed* right now?  'cause I am.

"replaced with other pattern-related methods" my ass.
*/
pub trait SubsliceOffset {
    /**
    Returns the byte offset of an inner slice relative to an enclosing outer slice.

    Examples

    ```ignore
    let string = "a\nb\nc";
    let lines: Vec<&str> = string.lines().collect();

    assert!(string.subslice_offset(lines[0]) == Some(0)); // &"a"
    assert!(string.subslice_offset(lines[1]) == Some(2)); // &"b"
    assert!(string.subslice_offset(lines[2]) == Some(4)); // &"c"
    assert!(string.subslice_offset("other!") == None);
    ```
    */
    fn subslice_offset_stable(&self, inner: &Self) -> Option<usize>;
}

impl SubsliceOffset for str {
    fn subslice_offset_stable(&self, inner: &str) -> Option<usize> {
        let self_beg = self.as_ptr() as usize;
        let inner = inner.as_ptr() as usize;
        if inner < self_beg || inner > self_beg.wrapping_add(self.len()) {
            None
        } else {
            Some(inner.wrapping_sub(self_beg))
        }
    }
}

