pub use exception_handler::debug_print;
use parking_lot::{Condvar, Mutex};
use std::{mem, sync::Arc};

#[allow(dead_code)]
pub enum ExceptionKind {
    Abort,
    Bus,
    Fpe,
    Illegal,
    InvalidParam,
    Purecall,
    SigSegv,
    StackOverflow,
    Trap,
}

cfg_if::cfg_if! {
    if #[cfg(unix)] {
        mod unix;
        pub use unix::*;
    } else if #[cfg(windows)] {
        mod windows;
        pub use windows::*;
    }
}
