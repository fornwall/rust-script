/*
Copyright ⓒ 2015-2017 cargo-script contributors.

Licensed under the MIT license (see LICENSE or <http://opensource.org
/licenses/MIT>) or the Apache License, Version 2.0 (see LICENSE of
<http://www.apache.org/licenses/LICENSE-2.0>), at your option. All
files in the project carrying such notice may not be copied, modified,
or distributed except according to those terms.
*/
/*!
This module contains the definition of the program's main error type.
*/

use std::borrow::Cow;
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
    Tag(Blame, Cow<'static, str>, Box<MainError>),
    Other(Blame, Box<dyn Error>),
    OtherOwned(Blame, String),
    OtherBorrowed(Blame, &'static str),
}

/**
Records who we have chosen to blame for a particular error.

This is used to distinguish between "report this to a user" and "explode violently".
*/
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Blame {
    Human,
    Internal,
}

impl MainError {
    pub fn blame(&self) -> Blame {
        use self::MainError::*;
        match *self {
            Io(blame, _)
            | Tag(blame, _, _)
            | Other(blame, _)
            | OtherOwned(blame, _)
            | OtherBorrowed(blame, _) => blame,
        }
    }

    pub fn is_human(&self) -> bool {
        self.blame() == Blame::Human
    }

    pub fn shift_blame(&mut self, blame: Blame) {
        use self::MainError::*;
        match *self {
            Io(ref mut cur_blame, _)
            | Tag(ref mut cur_blame, _, _)
            | Other(ref mut cur_blame, _)
            | OtherOwned(ref mut cur_blame, _)
            | OtherBorrowed(ref mut cur_blame, _) => *cur_blame = blame,
        }
    }
}

impl fmt::Display for MainError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> StdResult<(), fmt::Error> {
        use self::MainError::*;
        use std::fmt::Display;
        match *self {
            Io(_, ref err) => Display::fmt(err, fmt),
            Tag(_, ref msg, ref err) => write!(fmt, "{}: {}", msg, err),
            Other(_, ref err) => Display::fmt(err, fmt),
            OtherOwned(_, ref err) => Display::fmt(err, fmt),
            OtherBorrowed(_, ref err) => Display::fmt(err, fmt),
        }
    }
}

impl Error for MainError {}

macro_rules! from_impl {
    ($src_ty:ty => $dst_ty:ty, $src:ident -> $e:expr) => {
        impl From<$src_ty> for $dst_ty {
            fn from($src: $src_ty) -> $dst_ty {
                $e
            }
        }
    };
}

from_impl! { (Blame, io::Error) => MainError, v -> MainError::Io(v.0, v.1) }
from_impl! { (Blame, String) => MainError, v -> MainError::OtherOwned(v.0, v.1) }
from_impl! { (Blame, &'static str) => MainError, v -> MainError::OtherBorrowed(v.0, v.1) }
from_impl! { io::Error => MainError, v -> MainError::Io(Blame::Internal, v) }
from_impl! { String => MainError, v -> MainError::OtherOwned(Blame::Internal, v) }
from_impl! { &'static str => MainError, v -> MainError::OtherBorrowed(Blame::Internal, v) }

impl<T> From<Box<T>> for MainError
where
    T: 'static + Error,
{
    fn from(src: Box<T>) -> Self {
        MainError::Other(Blame::Internal, src)
    }
}

pub trait ResultExt {
    type Ok;
    fn err_tag<S>(self, msg: S) -> Result<Self::Ok>
    where
        S: Into<Cow<'static, str>>;

    fn shift_blame(self, blame: Blame) -> Self;
}

impl<T> ResultExt for Result<T> {
    type Ok = T;

    fn err_tag<S>(self, msg: S) -> Result<T>
    where
        S: Into<Cow<'static, str>>,
    {
        match self {
            Ok(v) => Ok(v),
            Err(e) => Err(MainError::Tag(e.blame(), msg.into(), Box::new(e))),
        }
    }

    fn shift_blame(self, blame: Blame) -> Self {
        match self {
            Ok(v) => Ok(v),
            Err(mut e) => {
                e.shift_blame(blame);
                Err(e)
            }
        }
    }
}
