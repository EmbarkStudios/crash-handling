//! Task exception ports are inherited by child processes, so a child spawned
//! after the handler is attached sends its exceptions to our handler thread.
//! These must be passed on to the kernel rather than claimed as handled, or the
//! child will re-execute the faulting instruction forever instead of receiving
//! the equivalent signal.
#![cfg(target_os = "macos")]
#![allow(unsafe_code)]

use crash_handler as ch;
use std::{
    os::unix::process::ExitStatusExt,
    process::{Command, ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

/// Environment variable that tells the re-executed test binary which crash to
/// perform as a child process
const CHILD_CASE: &str = "CRASH_HANDLER_FOREIGN_TASK_CASE";
/// Exit code of a child that did not inherit the parent's exception port,
/// which would make the test pass vacuously
const EXIT_NOT_INHERITED: i32 = 3;
/// Exit code of a child whose own signal handler caught the crash
const EXIT_CAUGHT: i32 = 42;
/// How long to wait for a child to die before deciding it is spinning
const DEADLINE: Duration = Duration::from_secs(10);

static PARENT_CALLBACK_INVOKED: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
enum Expected {
    Signal(&'static [i32]),
    Exit(i32),
}

impl Expected {
    fn matches(&self, status: ExitStatus) -> bool {
        match self {
            Self::Signal(signals) => status.signal().is_some_and(|sig| signals.contains(&sig)),
            Self::Exit(code) => status.code() == Some(*code),
        }
    }
}

#[test]
fn passes_on_foreign_task_exceptions() {
    if let Ok(case) = std::env::var(CHILD_CASE) {
        run_child(&case);
    }

    let _handler = unsafe {
        ch::CrashHandler::attach(ch::make_crash_event(|_cc: &ch::CrashContext| {
            PARENT_CALLBACK_INVOKED.store(true, Ordering::SeqCst);
            ch::CrashEventResult::Handled(false)
        }))
        .unwrap()
    };

    let cases = [
        ("segv", Expected::Signal(&[libc::SIGSEGV, libc::SIGBUS])),
        ("sigcatch", Expected::Exit(EXIT_CAUGHT)),
        ("illegal", Expected::Signal(&[libc::SIGILL])),
        ("trap", Expected::Signal(&[libc::SIGTRAP])),
    ];

    let dead_names_before = dead_port_names();

    let failures: Vec<_> = cases
        .iter()
        .filter_map(|(case, expected)| match spawn_child(case) {
            Some(status) if status.code() == Some(EXIT_NOT_INHERITED) => Some(format!(
                "{case}: child did not inherit the handler's exception port, so the test is vacuous"
            )),
            Some(status) if expected.matches(status) => None,
            Some(status) => Some(format!("{case}: expected {expected:?}, got {status}")),
            None => Some(format!(
                "{case}: child was still running after {DEADLINE:?} and was killed"
            )),
        })
        .collect();

    assert!(failures.is_empty(), "{failures:#?}");
    assert!(
        !PARENT_CALLBACK_INVOKED.load(Ordering::SeqCst),
        "the parent's crash callback was invoked for a child's exception"
    );
    // Each exception message carries send rights for the child's task and
    // thread, which become dead names once the child exits unless released
    assert_eq!(
        dead_names_before,
        dead_port_names(),
        "port rights received with the children's exceptions were leaked"
    );
}

/// Re-executes this test as a child process that performs the specified crash,
/// returning `None` if it has not exited by the deadline
fn spawn_child(case: &str) -> Option<ExitStatus> {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "passes_on_foreign_task_exceptions",
            "--exact",
            "--nocapture",
        ])
        .env(CHILD_CASE, case)
        .stdout(Stdio::null())
        .spawn()
        .expect("failed to spawn child");

    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("failed to wait on child") {
            return Some(status);
        }

        if start.elapsed() > DEADLINE {
            let _res = child.kill();
            let _res = child.wait();
            return None;
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_child(case: &str) -> ! {
    // SAFETY: syscalls, and crashing on purpose
    unsafe {
        if !inherited_exception_port() {
            #[allow(clippy::exit)]
            std::process::exit(EXIT_NOT_INHERITED);
        }

        match case {
            "segv" => sadness_generator::raise_segfault(),
            "sigcatch" => {
                // The autoconf "can segmentation violations be caught" check
                libc::signal(libc::SIGSEGV, exit_caught as *const () as usize);
                libc::signal(libc::SIGBUS, exit_caught as *const () as usize);
                sadness_generator::raise_segfault()
            }
            "illegal" => sadness_generator::raise_illegal_instruction(),
            "trap" => sadness_generator::raise_trap(),
            unknown => panic!("unknown case {unknown}"),
        }
    }
}

extern "C" fn exit_caught(_signal: i32) {
    // SAFETY: syscall, async signal safe
    unsafe { libc::_exit(EXIT_CAUGHT) };
}

/// Checks that this task has a `EXC_BAD_ACCESS` port with the behavior that
/// `CrashHandler::attach` uses, ie. the one inherited from the parent
///
/// SAFETY: syscalls
unsafe fn inherited_exception_port() -> bool {
    use mach2::{
        exception_types as et, kern_return::KERN_SUCCESS, port::MACH_PORT_NULL,
        traps::mach_task_self,
    };

    /// `EXC_TYPES_COUNT`
    const COUNT: usize = 14;

    let mut count = COUNT as u32;
    let mut masks = [0; COUNT];
    let mut ports = [0; COUNT];
    let mut behaviors = [0; COUNT];
    let mut flavors = [0; COUNT];

    let kr = unsafe {
        mach2::task::task_get_exception_ports(
            mach_task_self(),
            et::EXC_MASK_BAD_ACCESS,
            masks.as_mut_ptr(),
            &mut count,
            ports.as_mut_ptr(),
            behaviors.as_mut_ptr(),
            flavors.as_mut_ptr(),
        )
    };

    kr == KERN_SUCCESS
        && (0..count as usize).any(|i| {
            masks[i] & et::EXC_MASK_BAD_ACCESS != 0
                && ports[i] != MACH_PORT_NULL
                && behaviors[i] as u32 == et::EXCEPTION_DEFAULT | et::MACH_EXCEPTION_CODES
        })
}

/// Counts the dead names in this task's port space
fn dead_port_names() -> usize {
    use mach2::{
        kern_return::{KERN_SUCCESS, kern_return_t},
        mach_types::task_t,
        message::mach_msg_type_number_t,
        port::{MACH_PORT_RIGHT_DEAD_NAME, mach_port_name_t, mach_port_type_t},
        traps::mach_task_self,
        vm::mach_vm_deallocate,
    };

    /// `MACH_PORT_TYPE(MACH_PORT_RIGHT_DEAD_NAME)` from `<mach/port.h>`
    const MACH_PORT_TYPE_DEAD_NAME: mach_port_type_t = 1 << (MACH_PORT_RIGHT_DEAD_NAME + 16);

    unsafe extern "C" {
        /// Returns the names and types of all rights in the task's port space,
        /// in arrays allocated in the task that must be deallocated by the caller
        fn mach_port_names(
            task: task_t,
            names: *mut *mut mach_port_name_t,
            names_count: *mut mach_msg_type_number_t,
            types: *mut *mut mach_port_type_t,
            types_count: *mut mach_msg_type_number_t,
        ) -> kern_return_t;
    }

    // SAFETY: syscalls
    unsafe {
        let mut names = std::ptr::null_mut();
        let mut names_count = 0;
        let mut types = std::ptr::null_mut();
        let mut types_count = 0;

        assert_eq!(
            mach_port_names(
                mach_task_self(),
                &mut names,
                &mut names_count,
                &mut types,
                &mut types_count,
            ),
            KERN_SUCCESS
        );

        let dead = std::slice::from_raw_parts(types, types_count as usize)
            .iter()
            .filter(|ty| **ty & MACH_PORT_TYPE_DEAD_NAME != 0)
            .count();

        mach_vm_deallocate(
            mach_task_self(),
            names as _,
            (names_count as usize * size_of::<mach_port_name_t>()) as _,
        );
        mach_vm_deallocate(
            mach_task_self(),
            types as _,
            (types_count as usize * size_of::<mach_port_type_t>()) as _,
        );

        dead
    }
}
