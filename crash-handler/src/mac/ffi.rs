//! Due to Apple's ~fucking~ terrible documentation, these structures/code where
//! mainly copied from wasmtime's Mac traphandler (which themselves were
//! primarily copied from Spider Monkey) and Breakpad. I just wish Apple had
//! enough resources to properly document their own APIs OH WAIT. I was going
//! to clean this comment up when I was less salty, but turns out I'm still
//! salty so it's staying up.
//!
//! All the bindings here are lifted from headers in usr/include/mach, each one
//! notes the specific header it can be located in

pub use mach2::{
    exception_types as et,
    kern_return::{kern_return_t, KERN_SUCCESS},
    mach_init::mach_thread_self,
    mach_port as mp, mach_types as mt, message as msg,
    port::{self, mach_port_t, MACH_PORT_NULL},
    task, thread_status as ts,
    traps::mach_task_self,
};

/// Number of top level exception types
///
/// This is platform independent, but located the `<arch>/exception.h`
pub const EXC_TYPES_COUNT: usize = 14;
/// For `EXC_SOFTWARE` exceptions, this indicates the exception was due to a Unix signal
///
/// The actual Unix signal is stored in the subcode of the exception
///
/// `exception_types.h`
pub const EXC_SOFT_SIGNAL: u32 = 0x10003;
cfg_if::cfg_if! {
    if #[cfg(any(target_arch = "x86", target_arch = "x86_64"))] {
        pub const THREAD_STATE_NONE: ts::thread_state_flavor_t = 13;
    } else if #[cfg(any(target_arch = "arm", target_arch = "aarch64"))] {
        pub const THREAD_STATE_NONE: ts::thread_state_flavor_t = 5;
    }
}

// /// Machine-independent exception behaviors. Possible values for [`et::exception_behavior_t`].
// ///
// /// `exception_types.h`
// #[repr(i32)]
// pub enum ExceptionBehaviors {
//     /// Send a `catch_exception_raise` message including the identity.
//     Default = 1,
//     /// Send a `catch_exception_raise_state` message including the thread state.
//     State = 2,
//     /// Send a `catch_exception_raise_state_identity` message including the thread identity and state.
//     StateIdentity = 3,
//     /// Send a catch_exception_raise message including protected task and thread identity.
//     IdentityProtected = 4,
// }

/// Network Data Representation Record
///
/// ndr.h
#[repr(C)]
#[derive(Copy, Clone)]
pub struct NDR_record_t {
    pub mig_vers: u8,
    pub if_vers: u8,
    pub reserved1: u8,
    pub mig_encoding: u8,
    pub int_rep: u8,
    pub char_rep: u8,
    pub float_rep: u8,
    pub reserved2: u8,
}

/// These structures and techniques are illustrated in Mac OS X Internals, Amit Singh, ch 9.7
#[repr(C)]
pub struct ExceptionMessage {
    pub header: msg::mach_msg_header_t,
    pub body: msg::mach_msg_body_t,
    pub thread: msg::mach_msg_port_descriptor_t,
    pub task: msg::mach_msg_port_descriptor_t,
    pub ndr: NDR_record_t,
    pub exception: et::exception_type_t,
    pub code_count: msg::mach_msg_type_number_t,
    pub code: [i64; 2],
    pub padding: [u8; 512],
}

/// Whenever MIG detects an error, it sends back a generic `mig_reply_error_t`
/// format message.  Clients must accept these in addition to the expected reply
/// message format.
///
/// `mig_errors.h`
#[repr(C)]
pub struct ExceptionRaiseReply {
    pub header: msg::mach_msg_header_t,
    pub ndr: NDR_record_t,
    pub ret_code: kern_return_t,
}

extern "C" {
    /// Atomically (I assume) swaps the currently registered exception ports
    /// with a new one, returning the previously registered ports so that
    /// they can be restored later.
    ///
    /// Given the order of arguments I'm assuming this function has evolved
    /// over time, but basically (at least, according to how it is used in
    /// Breakpad), the output of this function will be 4 distinct arrays,
    /// which are basically a structure of arrays
    ///
    /// task.h
    pub fn task_swap_exception_ports(
        task: mt::task_t,                      // The task we want to swap the ports for
        exception_mask: et::exception_mask_t, // The mask of exceptions, will only swaps ports that match an exception in the mask
        new_port: mach_port_t,                // The new exception port we want to use
        behavior: et::exception_behavior_t,   // The exception behavior when sending to the port
        new_flavor: ts::thread_state_flavor_t, // What CPU context info to retrieve
        masks: *mut et::exception_mask_t, // Output array of each exception mask that has a registered port
        masks_count: *mut u32, // The length of the masks array, as well as the following arrays
        old_handlers: *mut mach_port_t, // Output array of ports that are registered
        old_behaviors: *mut et::exception_behavior_t, // Output array of behaviors
        old_flavors: *mut ts::thread_state_flavor_t, // Output array of thread flavors
    ) -> kern_return_t;

    /// Sets a new exception port for the specified exception.
    ///
    /// task.h
    pub fn task_set_exception_ports(
        task: mt::task_t,                      // The task we want to set the port for
        exception_mask: et::exception_mask_t,  // The exception we want to set the port for
        new_port: mach_port_t,                 // The new port to receive exceptions on
        behavior: et::exception_behavior_t,    // The exception behavior when send to the port
        new_flavor: ts::thread_state_flavor_t, // What CPU context info to send with the exception
    ) -> kern_return_t;

    /// The host? NDR
    ///
    /// <arch>/ndr_def.h
    pub static NDR_record: NDR_record_t;
}
