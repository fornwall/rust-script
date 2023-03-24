#[derive(Copy, Clone, Debug)]
pub enum BuildKind {
    Normal,
    Test,
    Bench,
}

impl BuildKind {
    pub const fn exec_command(&self) -> &'static str {
        match *self {
            Self::Normal => "build",
            Self::Test => "test",
            Self::Bench => "bench",
        }
    }

    pub fn from_flags(test: bool, bench: bool) -> Self {
        match (test, bench) {
            (false, false) => Self::Normal,
            (true, false) => Self::Test,
            (false, true) => Self::Bench,
            _ => panic!("got both test and bench"),
        }
    }
}
