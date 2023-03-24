/*!
Definition of the program's main error type.
*/

use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use std::io;
use std::result::Result;

/// Shorthand for the program's common result type.
pub type MainResult<T> = Result<T, MainError>;

/// An error in the program.
#[derive(Debug)]
pub enum MainError {
    Io(io::Error),
    Tag(Cow<'static, str>, Box<MainError>),
    Other(Box<dyn Error>),
    OtherOwned(String),
    OtherBorrowed(&'static str),
}

impl fmt::Display for MainError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        use self::MainError::*;
        use std::fmt::Display;
        match self {
            Io(err) => Display::fmt(err, fmt),
            Tag(msg, ref err) => write!(fmt, "{}: {}", msg, err),
            Other(err) => Display::fmt(err, fmt),
            OtherOwned(err) => Display::fmt(err, fmt),
            OtherBorrowed(err) => Display::fmt(err, fmt),
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

from_impl! { io::Error => MainError, v -> MainError::Io(v) }
from_impl! { String => MainError, v -> MainError::OtherOwned(v) }
from_impl! { &'static str => MainError, v -> MainError::OtherBorrowed(v) }

impl<T> From<Box<T>> for MainError
where
    T: 'static + Error,
{
    fn from(src: Box<T>) -> Self {
        Self::Other(src)
    }
}
