/*!
This module just contains other random implementation stuff.
*/
use std::error::Error;
use std::marker::PhantomData;

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

use std::str::pattern::{Pattern, Searcher, SearchStep};

pub trait ToMultiPattern<'a, P>
where P: Pattern<'a> {
    fn to_multi_pattern(self) -> MultiPattern<'a, P>;
}

impl<'a, P> ToMultiPattern<'a, P> for Vec<P>
where P: Pattern<'a> {
    fn to_multi_pattern(self) -> MultiPattern<'a, P> {
        MultiPattern::new(self)
    }
}

/**
Used to search against multiple patterns in a single pass.
*/
pub struct MultiPattern<'a, P>(Vec<P>, PhantomData<&'a P>)
where P: 'a + Pattern<'a>;

impl<'b, P> MultiPattern<'b, P>
where P: 'b + Pattern<'b> {
    pub fn new(sub_patterns: Vec<P>) -> MultiPattern<'b, P> {
        MultiPattern(sub_patterns, PhantomData)
    }
}

impl<'b, P> Pattern<'b> for MultiPattern<'b, P>
where P: 'b + Pattern<'b> {
    type Searcher = MultiPatternSearcher<'b, P::Searcher>;

    fn into_searcher(self, haystack: &'b str) -> MultiPatternSearcher<'b, P::Searcher> {
        MultiPatternSearcher {
            haystack: haystack,
            next_offset: 0,
            searchers: self.0.into_iter().map(|p| (0, p.into_searcher(haystack))).collect()
        }
    }
}

pub struct MultiPatternSearcher<'a, S>
where S: Searcher<'a> {
    haystack: &'a str,
    next_offset: usize,
    searchers: Vec<(usize, S)>,
}

unsafe impl<'a, S> Searcher<'a> for MultiPatternSearcher<'a, S>
where S: Searcher<'a> {
    fn haystack(&self) -> &'a str {
        self.haystack
    }

    fn next(&mut self) -> SearchStep {
        use std::str::pattern::SearchStep::*;

        let offset = self.next_offset;
        let haystack_len = self.haystack.len();
        let mut result = Done;

        if offset == haystack_len {
            return Done;
        }

        // next search offset, searcher
        for &mut (ref mut nso, ref mut s) in &mut self.searchers {
            if *nso > offset {
                // Skip for now.
                continue;
            }

            let s_result = s.next();

            match &s_result {
                &Match(_, b) | &Reject(_, b) => *nso = b,
                &Done => *nso = haystack_len
            }

            result = match (s_result, result) {
                (Match(a, b), Match(i, j)) if a < i || a == i && b > j => Match(a, b),
                (Match(_, _), Match(i, j)) => Match(i, j),
                (Match(a, b), Reject(_, _)) => Match(a, b),
                (Match(a, b), Done) => Match(a, b),

                (Reject(_, _), Match(i, j)) => Match(i, j),
                (Reject(a, b), Reject(i, j)) if a < i || a == i && b > j => Reject(a, b),
                (Reject(_, _), Reject(i, j)) => Reject(i, j),
                (Reject(a, b), Done) => Reject(a, b),

                (Done, result) => result,
            };
        }

        self.next_offset = match &result {
            &Match(_, b) | &Reject(_, b) => b,
            &Done => haystack_len
        };

        result
    }
}
