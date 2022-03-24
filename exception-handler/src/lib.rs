// BEGIN - Embark standard lints v6 for Rust 1.55+
// do not change or add/remove here, but one can add exceptions after this section
// for more info see: <https://github.com/EmbarkStudios/rust-ecosystem/issues/59>
#![deny(unsafe_code)]
#![warn(
    clippy::all,
    clippy::await_holding_lock,
    clippy::char_lit_as_u8,
    clippy::checked_conversions,
    clippy::dbg_macro,
    clippy::debug_assert_with_mut_call,
    clippy::doc_markdown,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::exit,
    clippy::expl_impl_clone_on_copy,
    clippy::explicit_deref_methods,
    clippy::explicit_into_iter_loop,
    clippy::fallible_impl_from,
    clippy::filter_map_next,
    clippy::flat_map_option,
    clippy::float_cmp_const,
    clippy::fn_params_excessive_bools,
    clippy::from_iter_instead_of_collect,
    clippy::if_let_mutex,
    clippy::implicit_clone,
    clippy::imprecise_flops,
    clippy::inefficient_to_string,
    clippy::invalid_upcast_comparisons,
    clippy::large_digit_groups,
    clippy::large_stack_arrays,
    clippy::large_types_passed_by_value,
    clippy::let_unit_value,
    clippy::linkedlist,
    clippy::lossy_float_literal,
    clippy::macro_use_imports,
    clippy::manual_ok_or,
    clippy::map_err_ignore,
    clippy::map_flatten,
    clippy::map_unwrap_or,
    clippy::match_on_vec_items,
    clippy::match_same_arms,
    clippy::match_wild_err_arm,
    clippy::match_wildcard_for_single_variants,
    clippy::mem_forget,
    clippy::mismatched_target_os,
    clippy::missing_enforced_import_renames,
    clippy::mut_mut,
    clippy::mutex_integer,
    clippy::needless_borrow,
    clippy::needless_continue,
    clippy::needless_for_each,
    clippy::option_option,
    clippy::path_buf_push_overwrite,
    clippy::ptr_as_ptr,
    clippy::rc_mutex,
    clippy::ref_option_ref,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::same_functions_in_if_condition,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::string_add_assign,
    clippy::string_add,
    clippy::string_lit_as_bytes,
    clippy::string_to_string,
    clippy::todo,
    clippy::trait_duplication_in_bounds,
    clippy::unimplemented,
    clippy::unnested_or_patterns,
    clippy::unused_self,
    clippy::useless_transmute,
    clippy::verbose_file_reads,
    clippy::zero_sized_map_values,
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms
)]
// END - Embark standard lints v6 for Rust 1.55+
// crate-specific exceptions:
#![allow(unsafe_code)]

mod error;

pub use error::Error;

#[cfg(feature = "debug-print")]
#[macro_export]
macro_rules! debug_print {
    ($s:literal) => {
        let cstr = concat!($s, "\n");
        $crate::write_stderr(cstr);
    };
}

#[cfg(not(feature = "debug-print"))]
#[macro_export]
macro_rules! debug_print {
    ($s:literal) => {};
}

/// Writes the specified string directly to stderr.
///
/// This is safe to be called from within a compromised context.
#[inline]
pub fn write_stderr(s: &'static str) {
    unsafe {
        #[cfg(target_os = "windows")]
        libc::write(2, s.as_ptr().cast(), s.len() as u32);

        #[cfg(not(target_os = "windows"))]
        libc::write(2, s.as_ptr().cast(), s.len());
    }
}

cfg_if::cfg_if! {
    if #[cfg(unix)] {
        pub mod unix;
    }
}

pub use crash_context::CrashContext;

/// User implemented trait for handling a signal that has ocurred.
///
/// # Safety
///
/// This trait is marked unsafe as  care needs to be taken when implementing it
/// due to running in a compromised context. Notably, only a small subset of
/// libc functions are [async signal safe](https://man7.org/linux/man-pages/man7/signal-safety.7.html)
/// and calling non-safe ones can have undefined behavior, including such common
/// ones as `malloc` (if using a multi-threaded allocator). In general, it is
/// advised to do as _little_ as possible when handling a signal, with more
/// complicated or dangerous (in a compromised context) code being intialized
/// before the signal handler is installed, or hoisted out to an entirely
/// different sub-process.
pub unsafe trait CrashEvent: Send + Sync {
    /// Method invoked when a crash occurs. Returning true indicates your handler
    /// has processed the crash and that no further handlers should run.
    fn on_crash(&self, context: &CrashContext) -> bool;
}

/// Creates a [`CrashEvent`] using the supplied closure as the implementation.
///
/// # Safety
///
/// See the [`CrashEvent`] Safety section for information on why this is `unsafe`.
#[inline]
pub unsafe fn make_crash_event<F>(closure: F) -> Box<dyn CrashEvent>
where
    F: Send + Sync + Fn(&CrashContext) -> bool + 'static,
{
    struct Wrapper<F> {
        inner: F,
    }

    unsafe impl<F> CrashEvent for Wrapper<F>
    where
        F: Send + Sync + Fn(&CrashContext) -> bool,
    {
        fn on_crash(&self, context: &CrashContext) -> bool {
            (self.inner)(context)
        }
    }

    Box::new(Wrapper { inner: closure })
}

cfg_if::cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "android"))] {
        #[macro_use]
        pub mod linux;

        pub use linux::{ExceptionHandler, Signal};
    } else if #[cfg(target_os = "windows")] {
        #[macro_use]
        pub mod windows;

        pub use windows::{ExceptionHandler};
    }
}
