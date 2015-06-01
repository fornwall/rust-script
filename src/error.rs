/*!
This module contains the definition of the program's main error type.
*/

use std::error::Error;
use std::fmt;
use std::io;
use std::result::Result as StdResult;

/**
Shorthand for the program's common result type.
*/
pub type Result<T> = StdResult<T, MainError>;

/**
Represents an error in the program.
*/
#[derive(Debug)]
pub enum MainError {
    Io(Blame, io::Error),
    OtherOwned(Blame, String),
    OtherBorrowed(Blame, &'static str),
}

/**
Records who we have chosen to blame for a particular error.

This is used to distinguish between "report this to a user" and "explode violently".
*/
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Blame { Human, Internal }

impl MainError {
    pub fn is_human(&self) -> bool {
        use self::MainError::*;
        match *self {
            Io(blame, _)
            | OtherOwned(blame, _)
            | OtherBorrowed(blame, _) => blame == Blame::Human,
        }
    }
}

impl fmt::Display for MainError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> StdResult<(), fmt::Error> {
        use self::MainError::*;
        use std::fmt::Display;
        match *self {
            Io(_, ref err) => Display::fmt(err, fmt),
            OtherOwned(_, ref err) => Display::fmt(err, fmt),
            OtherBorrowed(_, ref err) => Display::fmt(err, fmt),
        }
    }
}

impl Error for MainError {
    fn description(&self) -> &str {
        use self::MainError::*;
        match *self {
            Io(_, ref err) => err.description(),
            OtherOwned(_, ref err) => err,
            OtherBorrowed(_, ref err) => err,
        }
    }
}

macro_rules! from_impl {
    ($src_ty:ty => $dst_ty:ty, $src:ident -> $e:expr) => {
        impl From<$src_ty> for $dst_ty {
            fn from($src: $src_ty) -> $dst_ty {
                $e
            }
        }
    }
}

from_impl! { (Blame, io::Error) => MainError, v -> MainError::Io(v.0, v.1) }
from_impl! { (Blame, String) => MainError, v -> MainError::OtherOwned(v.0, v.1) }
from_impl! { (Blame, &'static str) => MainError, v -> MainError::OtherBorrowed(v.0, v.1) }
from_impl! { io::Error => MainError, v -> MainError::Io(Blame::Internal, v) }
from_impl! { String => MainError, v -> MainError::OtherOwned(Blame::Internal, v) }
from_impl! { &'static str => MainError, v -> MainError::OtherBorrowed(Blame::Internal, v) }
