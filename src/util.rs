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
    where
        F: FnOnce(Self) -> Self,
    {
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
where
    F: 'a + FnOnce() -> Result<(), E>,
    E: Error;

impl<'a, F, E> Defer<'a, F, E>
where
    F: 'a + FnOnce() -> Result<(), E>,
    E: Error,
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
    E: Error,
{
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            if let Err(err) = f() {
                error!("deferred function failed: {}", err);
            }
        }
    }
}

pub use self::suppress_child_output::{suppress_child_output, ChildToken};

mod suppress_child_output {
    use crate::error::Result;

    use std::io;
    use std::process::{self, Command};
    use std::thread;

    /**
    Suppresses the stderr output of a child process, unless:

    - the process exits and signals a failure.

    If so, the existing output is flushes to the current process' stderr, and all further output from the child is passed through.

    In other words: if the child successfully completes, it's stderr output is suppressed.  Otherwise, it's let through.
    */
    pub fn suppress_child_output(cmd: &mut Command) -> Result<ChildToken> {
        cmd.stderr(process::Stdio::piped());

        let mut child = cmd.spawn()?;
        let stderr = child.stderr.take().expect("no stderr pipe found");

        let (done_sig, done_gate) = crossbeam_channel::bounded(0);

        let _ = thread::spawn(move || {
            let show_stderr;
            select! {
                recv(done_gate) -> success => {
                    show_stderr = !success.unwrap_or(true);
                },
            }
            if show_stderr {
                let mut stderr = stderr;
                io::copy(&mut stderr, &mut io::stderr()).expect("could not copy child stderr");
            }
        });

        Ok(ChildToken {
            child,
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
                    return Err(e);
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
