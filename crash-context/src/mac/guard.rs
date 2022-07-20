//! Contains types and helpers for dealing with `EXC_GUARD` exceptions.
//!
//! `EXC_GUARD` exceptions embed details about the guarded resource in the `code`
//! and `subcode` fields of the exception
//!
//! See <https://github.com/apple-oss-distributions/xnu/blob/e7776783b89a353188416a9a346c6cdb4928faad/osfmk/kern/exc_guard.h>
//! for the top level types that this module wraps.

use mach2::exception_types::EXC_GUARD;

/// The set of possible guard kinds
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum GuardKind {
    /// Null variant
    None = 0,
    /// A `mach_port_t`
    MachPort = 1,
    /// File descriptor
    Fd = 2,
    /// Userland assertion
    User = 3,
    /// Vnode
    Vnode = 4,
    /// Virtual memory operation
    VirtualMemory = 5,
    /// Rejected system call trap
    RejectedSyscall = 6,
}

#[inline]
pub fn extract_guard_kind(code: i64) -> u8 {
    ((code >> 61) & 0x7) as u8
}

#[inline]
pub fn extract_guard_flavor(code: i64) -> u32 {
    ((code >> 32) & 0x1fffffff) as u32
}

#[inline]
pub fn extract_guard_target(code: i64) -> u32 {
    code as u32
}

/// The extracted details of an `EXC_GUARD` exception
pub struct GuardException {
    /// One of [`GuardKind`]
    pub kind: u8,
    /// The specific guard flavor that was violated, specific to each `kind`
    pub flavor: u32,
    /// The resource that was guarded
    pub target: u32,
    /// Target specific guard information
    pub identifier: u64,
}

/// Extracts the guard details from an exceptions code and subcode
///
/// code:
/// +-------------------+----------------+--------------+
/// |[63:61] guard type | [60:32] flavor | [31:0] target|
/// +-------------------+----------------+--------------+
///
/// subcode:
/// +---------------------------------------------------+
/// |[63:0] guard identifier                            |
/// +---------------------------------------------------+
#[inline]
pub fn extract_guard_exception(code: i64, subcode: i64) -> GuardException {
    GuardDetails {
        kind: extract_guard_kind(code),
        flavor: extract_guard_flavor(code),
        target: extract_guard_target(code),
        identifier: subcode as u64,
    }
}

impl super::ExceptionInfo {
    /// If this is an `EXC_GUARD` exception, retrieves the exception metadata
    /// from the code, otherwise returns `None`
    pub fn guard_exception(&self) -> Option<GuardException> {
        if self.kind as u32 != EXC_GUARD {
            return None;
        }

        Some(extract_guard_exception(
            self.code,
            self.subcode.unwrap_or_default(),
        ))
    }
}

// /// Mach port guard flavors
// ///
// /// [Kernel source](https://github.com/apple-oss-distributions/xnu/blob/e6231be02a03711ca404e5121a151b24afbff733/osfmk/mach/port.h#L469-L496)
// #[derive(Copy, Clone, PartialEq, Debug)]
// #[repr(u32)]
// pub enum MachPortFlavors {
//     // Fatal guards
//     Destroy = 1 << 0,
//     ModRefs = 1 << 1,
//     SetContext = 1 << 2,
//     Unguarded = 1 << 3,
//     IncorrectGuard = 1 << 4,
//     Immovable = 1 << 5,
//     StrictReply = 1 << 6,
//     MsgFiltered = 1 << 7,

//     // Optionally fatal guards
//     InvalidRight = 1 << 8,
//     InvalidName = 1 << 9,
//     InvalidValue = 1 << 10,
//     InvalidArgument = 1 << 11,
//     RightExists = 1 << 12,
//     KernNoSpace = 1 << 13,
//     KernFailure = 1 << 14,
//     KernResource = 1 << 15,
//     SendInvalidReply = 1 << 16,
//     SendInvalidVoucher = 1 << 17,
//     SendInvalidRight = 1 << 18,
//     ReceiveInvalidName = 1 << 19,

//     // Non-fatal guards
//     ReceiveGuardedDesc = 1 << 20,
//     ModRefsNonFatal = 1 << 1,
// }

// /// Mach port guards can be either always, never, or optionally fatal
// #[derive(Copy, Clone PartialEq, Debug)]
// pub enum Fatal {
//     Yes,
//     No,
//     Optional,
// }

// impl MachPortFlavors {
//     /// Retrieves whether the exception is fatal or not
//     pub fn fatal(self) -> Fatal {
//         if self as u32 <= Self::MsgFiltered as u32 {
//             Fatal::Yes
//         } else if self as u32 >= Self::ReceiveGuardedDesc as u32 {
//             Fatal::No
//         } else {
//             Fatal::Optional
//         }
//     }
// }

// pub struct MachPortException {}
