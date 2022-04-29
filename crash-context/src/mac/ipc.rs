//! Unfortunately, sending a [`CrashContext`] to another process on Macos
//! needs to be done via mach ports, as, for example, `mach_task_self` is a
//! special handle that needs to be translated into the "actual" task when used
//! by another process, this _might_ be possible completely in userspace, but
//! examining the source code for this leads me to believe that there are enough
//! footguns, particularly around security, that this might take a while, so for
//! now, if you need to use a [`CrashContext`] across processes, you need
//! to use the IPC mechanisms here to get meaningful/accurate data
//!
//! Note that in all cases of an optional timeout, a `None` will return
//! immediately regardless of whether the messaged has been enqueued or
//! dequeued from the kernel queue, so it is _highly_ recommended to use
//! reasonable timeouts for sending and receiving messages between processes.

use crate::CrashContext;
use mach2::{
    bootstrap, kern_return::KERN_SUCCESS, mach_port, message as msg, port, task,
    traps::mach_task_self,
};
pub use mach2::{kern_return::kern_return_t, message::mach_msg_return_t};
use std::{ffi::CStr, time::Duration};

extern "C" {
    /// From <usr/include/mach/mach_traps.h>, there is no binding for this in mach2
    pub fn pid_for_task(task: port::mach_port_name_t, pid: *mut i32) -> kern_return_t;
}

/// The actual crash context message sent and received. This message is a single
/// struct since it needs to be contiguous block of memory. I suppose it's like
/// this because people are expected to use MIG to generate the interface code,
/// but it's ugly as hell regardless.
#[repr(C)]
#[derive(Debug)]
struct CrashContextMessage {
    head: msg::mach_msg_header_t,
    /// When providing port descriptors, this must be present to say how many
    /// of them follow the header and body
    body: msg::mach_msg_body_t,
    // These are the really the critical piece of the payload, during
    // sending (or receiving?) these are turned into descriptors that
    // can actually be used by another process
    /// The task that crashed (ie `mach_task_self`)
    task: msg::mach_msg_port_descriptor_t,
    /// The thread that crashed
    crash_thread: msg::mach_msg_port_descriptor_t,
    /// The handler thread, probably, but not necessarily `mach_thread_self`
    handler_thread: msg::mach_msg_port_descriptor_t,
    // Port opened by the client to receive an ack from the server
    ack_port: msg::mach_msg_port_descriptor_t,
    /// Combination of the FLAG_* constants
    flags: u32,
    /// The exception type
    exception_kind: i32,
    /// The exception code
    exception_code: i64,
    /// The optional exception subcode
    exception_subcode: i64,
}

const FLAG_HAS_EXCEPTION: u32 = 0x1;
const FLAG_HAS_SUBCODE: u32 = 0x2;

/// Message sent from the [`Receiver`] upon receiving and handling a [`CrashContextMessage`]
#[repr(C)]
struct AcknowledgementMessage {
    head: msg::mach_msg_header_t,
    result: u32,
}

/// An error that can occur while interacting with mach ports
#[derive(Copy, Clone, Debug)]
pub enum Error {
    /// A kernel error will generally indicate an error occurred while creating
    /// or modifying a mach port
    Kernel(kern_return_t),
    /// A message error indicates an error occurred while sending or receiving
    /// a message on a mach port
    Message(mach_msg_return_t),
}

impl std::error::Error for Error {}

use std::fmt;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO: use a good string for the error codes
        write!(f, "{:?}", self)
    }
}

macro_rules! kern {
    ($call:expr) => {{
        let res = $call;

        if res != KERN_SUCCESS {
            return Err(Error::Kernel(res));
        }
    }};
}

macro_rules! msg {
    ($call:expr) => {{
        let res = $call;

        if res != msg::MACH_MSG_SUCCESS {
            return Err(Error::Message(res));
        }
    }};
}

/// Sends a [`CrashContext`] from a crashing process to another process running
/// a [`Server`] with the same name
pub struct Client {
    port: port::mach_port_t,
}

impl Client {
    /// Attempts to create a new client that can send messages to a [`Server`]
    /// that was created with the specified name.
    ///
    /// # Errors
    ///
    /// The specified port is not available for some reason, if you expect the
    /// port to be created you can retry this function until it connects.
    pub fn create(name: &CStr) -> Result<Self, Error> {
        // SAFETY: syscalls. The user has no invariants to uphold, hence the
        // unsafe not being on the function as a whole
        unsafe {
            let mut task_bootstrap_port = 0;
            kern!(task::task_get_special_port(
                mach_task_self(),
                task::TASK_BOOTSTRAP_PORT,
                &mut task_bootstrap_port
            ));

            let mut port = 0;
            kern!(bootstrap::bootstrap_look_up(
                task_bootstrap_port,
                name.as_ptr(),
                &mut port
            ));

            Ok(Self { port })
        }
    }

    /// Sends the specified [`CrateContext`] to a [`Server`].
    ///
    /// If the ack from the [`Server`] times out `Ok(None)` is returned, otherwise
    /// it is the value specified in the [`Server`] process to [`Acknowledger::send_ack`]
    pub fn send_crash_context(
        &self,
        ctx: &CrashContext,
        send_timeout: Option<Duration>,
        receive_timeout: Option<Duration>,
    ) -> Result<Option<u32>, Error> {
        // SAFETY: syscalls. Again, the user has no invariants to uphold, so
        // the function itself is not marked unsafe
        unsafe {
            // Create a new port to receive a response from the reciving end of
            // this port so we that we know when it has actually processed the
            // CrashContext, which is (presumably) interesting for the caller. If
            // that is not interesting they can set the receive_timeout to 0 to
            // just return immediately
            let mut ack_port = AckReceiver::new()?;

            let (flags, exception_kind, exception_code, exception_subcode) =
                if let Some(exc) = ctx.exception {
                    (
                        FLAG_HAS_EXCEPTION
                            | if exc.subcode.is_some() {
                                FLAG_HAS_SUBCODE
                            } else {
                                0
                            },
                        exc.kind,
                        exc.code,
                        exc.subcode.unwrap_or_default(),
                    )
                } else {
                    (0, 0, 0, 0)
                };

            let mut msg = CrashContextMessage {
                head: msg::mach_msg_header_t {
                    msgh_bits: msg::MACH_MSG_TYPE_COPY_SEND | msg::MACH_MSGH_BITS_COMPLEX,
                    msgh_size: std::mem::size_of::<CrashContextMessage>() as u32,
                    msgh_remote_port: self.port,
                    msgh_local_port: port::MACH_PORT_NULL,
                    msgh_voucher_port: port::MACH_PORT_NULL,
                    msgh_id: 0,
                },
                body: msg::mach_msg_body_t {
                    msgh_descriptor_count: 4,
                },
                task: msg::mach_msg_port_descriptor_t::new(ctx.task, msg::MACH_MSG_TYPE_COPY_SEND),
                crash_thread: msg::mach_msg_port_descriptor_t::new(
                    ctx.thread,
                    msg::MACH_MSG_TYPE_COPY_SEND,
                ),
                handler_thread: msg::mach_msg_port_descriptor_t::new(
                    ctx.handler_thread,
                    msg::MACH_MSG_TYPE_COPY_SEND,
                ),
                ack_port: msg::mach_msg_port_descriptor_t::new(
                    ack_port.port,
                    msg::MACH_MSG_TYPE_COPY_SEND,
                ),
                flags,
                exception_kind,
                exception_code,
                exception_subcode,
            };

            // Try to actually send the message to the Server
            msg!(msg::mach_msg(
                &mut msg.head,
                msg::MACH_SEND_MSG | msg::MACH_SEND_TIMEOUT,
                msg.head.msgh_size,
                0,
                port::MACH_PORT_NULL,
                send_timeout
                    .map(|st| st.as_millis() as u32)
                    .unwrap_or_default(),
                port::MACH_PORT_NULL
            ));

            // Wait for a response from the Server
            match ack_port.recv_ack(receive_timeout) {
                Ok(result) => Ok(Some(result)),
                Err(Error::Message(msg::MACH_RCV_TIMED_OUT)) => Ok(None),
                Err(e) => Err(e),
            }
        }
    }
}

/// Returned from [`Server::try_recv_crash_context`] when a [`Client`] has sent
/// a crash context
pub struct ReceivedCrashContext {
    /// The crash context sent by a [`Client`]
    pub crash_context: CrashContext,
    /// Allows the sending of an ack back to the [`Client`] to acknowledge that
    /// your code has received and processed the [`CrashContext`]
    pub acker: Acknowledger,
    /// The process id of the process the [`Client`] lives in. This is retrieved
    /// via `pid_for_task`.
    pub pid: u32,
}

/// Receives a [`CrashContext`] from another process
pub struct Server {
    port: port::mach_port_t,
}

impl Server {
    /// Creates a new [`Server`] "bound" to the specified service name.
    pub fn create(name: &CStr) -> Result<Self, Error> {
        // SAFETY: syscalls. Again, the caller has no invariants to uphold, so
        // the entire function is not marked as unsafe
        unsafe {
            let mut task_bootstrap_port = 0;
            kern!(task::task_get_special_port(
                mach_task_self(),
                task::TASK_BOOTSTRAP_PORT,
                &mut task_bootstrap_port
            ));

            let mut port = 0;
            // Note that Breakpad uses bootstrap_register instead of this function as
            // MacOS 10.5 apparently deprecated bootstrap_register and then provided
            // bootstrap_check_in, but broken. However, 10.5 had its most recent update
            // over 13 years ago, and is not supported by Apple, so why should we?
            kern!(bootstrap::bootstrap_check_in(
                task_bootstrap_port,
                name.as_ptr(),
                &mut port,
            ));

            Ok(Self { port })
        }
    }

    /// Attempts to retrieve a [`CrashContext`] sent from a crashing process.
    ///
    /// Note that in event of a timeout, this method will return `Ok(None)` to
    /// indicate that a crash context was unavailable rather than an error.
    pub fn try_recv_crash_context(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<Option<ReceivedCrashContext>, Error> {
        // SAFETY: syscalls. The caller has no invariants to uphold, so the
        // entire function is not marked unsafe.
        unsafe {
            let mut crash_ctx_msg: CrashContextMessage = std::mem::zeroed();
            crash_ctx_msg.head.msgh_local_port = self.port;

            let ret = msg::mach_msg(
                &mut crash_ctx_msg.head,
                msg::MACH_RCV_MSG | msg::MACH_RCV_TIMEOUT,
                0,
                // So you may be thinking, wow, you are lying to the kernel about
                // the size of the buffer it can fill, this is terrible and you
                // should be ashamed, however, if we don't lie here, mach_msg will
                // return `MACH_RCV_TOO_LARGE`. I _think_ this might be because
                // the data payload that follows the header, body, and descriptors
                // needs to be 4-byte aligned or something? But regardless, if
                // we lie, the kernel only fills out the actual size of the message,
                // which is the real size of this struct and everything is happy.
                // Except me, because this is the kind of stuff that should be
                // documented, and the documentation that does exist (ie, not Apple's)
                // makes no mention of this, at least that I have found so far,
                // but of course, since there is no single source of truth the
                // "documentation" for this stuff is spread across random blog
                // posts and GNU documentation that probably comes from the 90s.
                // NOT SALTY AT ALL
                std::mem::size_of::<CrashContextMessage>() as u32 + 8,
                self.port,
                timeout.map(|t| t.as_millis() as u32).unwrap_or_default(),
                port::MACH_PORT_NULL,
            );

            if ret == msg::MACH_RCV_TIMED_OUT {
                return Ok(None);
            } else if ret != msg::MACH_MSG_SUCCESS {
                return Err(Error::Message(ret));
            }

            // Reconstruct a crash context from the message we received
            let exception = if crash_ctx_msg.flags & FLAG_HAS_EXCEPTION != 0 {
                Some(crate::ExceptionInfo {
                    kind: crash_ctx_msg.exception_kind,
                    code: crash_ctx_msg.exception_code,
                    subcode: (crash_ctx_msg.flags & FLAG_HAS_SUBCODE != 0)
                        .then(|| crash_ctx_msg.exception_subcode),
                })
            } else {
                None
            };

            let crash_context = CrashContext {
                task: crash_ctx_msg.task.name,
                thread: crash_ctx_msg.crash_thread.name,
                handler_thread: crash_ctx_msg.handler_thread.name,
                exception,
            };

            // Translate the task to a pid so the user doesn't have to do it
            // since there is not a binding available in libc/mach/mach2 for it
            let mut pid = 0;
            kern!(pid_for_task(crash_ctx_msg.task.name, &mut pid));
            let ack_port = crash_ctx_msg.ack_port.name;

            // Provide a way for the user to tell the client when they are done
            // processing the crash context, unless the specified port was not
            // set or somehow died immediately
            let acker = Acknowledger {
                port: (ack_port != port::MACH_PORT_DEAD && ack_port != port::MACH_PORT_NULL)
                    .then(|| ack_port),
            };

            Ok(Some(ReceivedCrashContext {
                crash_context,
                acker,
                pid: pid as u32,
            }))
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SAFETY: syscall
        unsafe {
            mach_port::mach_port_deallocate(mach_task_self(), self.port);
        }
    }
}

/// Used by a process running the [`Server`] to send a response back to the
/// [`Client`] that sent a [`CrashContext`] after it has finished
/// processing.
pub struct Acknowledger {
    port: Option<port::mach_port_t>,
}

impl Acknowledger {
    /// Sends an ack back to the client that sent a [`CrashContext`]
    pub fn send_ack(&mut self, ack: u32, timeout: Option<Duration>) -> Result<(), Error> {
        if let Some(port) = self.port {
            // SAFETY: syscalls. The caller has no invariants to uphold, so the
            // entire function is not marked unsafe.
            unsafe {
                let mut msg = AcknowledgementMessage {
                    head: msg::mach_msg_header_t {
                        msgh_bits: msg::MACH_MSG_TYPE_COPY_SEND,
                        msgh_size: std::mem::size_of::<AcknowledgementMessage>() as u32,
                        msgh_remote_port: port,
                        msgh_local_port: port::MACH_PORT_NULL,
                        msgh_voucher_port: port::MACH_PORT_NULL,
                        msgh_id: 0,
                    },
                    result: ack,
                };

                // Try to actually send the message
                msg!(msg::mach_msg(
                    &mut msg.head,
                    msg::MACH_SEND_MSG | msg::MACH_SEND_TIMEOUT,
                    msg.head.msgh_size,
                    0,
                    port::MACH_PORT_NULL,
                    timeout.map(|t| t.as_millis() as u32).unwrap_or_default(),
                    port::MACH_PORT_NULL
                ));

                Ok(())
            }
        } else {
            Ok(())
        }
    }
}

/// Used by [`Sender::send_crash_context`] to create a port to receive the
/// external process's response to sending a [`CrashContext`]
struct AckReceiver {
    port: port::mach_port_t,
}

impl AckReceiver {
    /// Allocates a new port to receive an ack from a [`Server`]
    ///
    /// SAFETY: syscalls. Only used internally hence the entire function being
    /// marked unsafe.
    unsafe fn new() -> Result<Self, Error> {
        let mut port = 0;
        kern!(mach_port::mach_port_allocate(
            mach_task_self(),
            port::MACH_PORT_RIGHT_RECEIVE,
            &mut port
        ));

        kern!(mach_port::mach_port_insert_right(
            mach_task_self(),
            port,
            port,
            msg::MACH_MSG_TYPE_MAKE_SEND
        ));

        Ok(Self { port })
    }

    /// Waits for the specified duration to receive a result from the [`Server`]
    /// that was sent a [`CrashContext`]
    ///
    /// SAFETY: syscalls. Only used internally hence the entire function being
    /// marked unsafe.
    unsafe fn recv_ack(&mut self, timeout: Option<Duration>) -> Result<u32, Error> {
        let mut ack = AcknowledgementMessage {
            head: msg::mach_msg_header_t {
                msgh_bits: 0,
                msgh_size: std::mem::size_of::<AcknowledgementMessage>() as u32,
                msgh_remote_port: port::MACH_PORT_NULL,
                msgh_local_port: self.port,
                msgh_voucher_port: port::MACH_PORT_NULL,
                msgh_id: 0,
            },
            result: 0,
        };

        // Wait for a response from the Server
        msg!(msg::mach_msg(
            &mut ack.head,
            msg::MACH_RCV_MSG | msg::MACH_RCV_TIMEOUT,
            0,
            ack.head.msgh_size,
            self.port,
            timeout.map(|t| t.as_millis() as u32).unwrap_or_default(),
            port::MACH_PORT_NULL
        ));

        Ok(ack.result)
    }
}

impl Drop for AckReceiver {
    fn drop(&mut self) {
        // SAFETY: syscall
        unsafe {
            mach_port::mach_port_deallocate(mach_task_self(), self.port);
        }
    }
}
