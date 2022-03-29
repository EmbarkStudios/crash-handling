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

mod errors;

pub use errors::Error;
use std::{fs::File, path::PathBuf};

cfg_if::cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "android"))] {
        mod linux;

        pub use linux::{Client, Server};
    } else if #[cfg(target_os = "windows")] {
        mod windows;

        pub use windows::{Client, Server};
    }
}

pub struct MinidumpBinary {
    /// The file the minidump was written to, as provided by [`ServerHandler::create_minidump_file`]
    pub file: File,
    /// The path to the file as provided by [`ServerHandler::create_minidump_file`].
    pub path: PathBuf,
    /// The in-memory contents of the minidump, may be empty
    pub contents: Vec<u8>,
}

/// Allows user code to hook into the server to avoid hardcoding too many details
pub trait ServerHandler: Send + Sync {
    /// Called when a crash request has been received and a backing file needs
    /// to be created to store it.
    fn create_minidump_file(&self) -> Result<(File, PathBuf), std::io::Error>;
    /// Called when a crash has been fully written as a minidump to the provided
    /// file. Also returns the full heap buffer as well.
    ///
    /// A return value of true indicates that the message loop should exit and
    /// stop processing messages.
    fn on_minidump_created(&self, result: Result<MinidumpBinary, Error>) -> bool;
    /// Called when the client sends a user message sent from the client with
    /// `send_message`
    fn on_message(&self, kind: u32, buffer: Vec<u8>);
    /// Optional allocation function for the buffer used to store a message.
    ///
    /// Defaults to creating a new vec.
    fn message_alloc(&self) -> Vec<u8> {
        Vec::new()
    }
}

#[derive(Copy, Clone)]
#[cfg_attr(test, derive(PartialEq, Debug))]
#[repr(C)]
pub(crate) struct Header {
    kind: u32,
    size: u32,
}

impl Header {
    fn as_bytes(&self) -> &[u8] {
        #[allow(unsafe_code)]
        unsafe {
            let size = std::mem::size_of::<Self>();
            let ptr = (self as *const Self).cast();
            std::slice::from_raw_parts(ptr, size)
        }
    }

    fn from_bytes(buf: &[u8]) -> Option<Self> {
        if buf.len() != std::mem::size_of::<Self>() {
            return None;
        }

        #[allow(unsafe_code)]
        unsafe {
            Some(*buf.as_ptr().cast::<Self>())
        }
    }
}

#[inline]
#[allow(unsafe_code)]
pub(crate) fn write_stderr(s: &'static str) {
    unsafe {
        libc::write(2, s.as_ptr().cast(), s.len() as _);
    }
}

#[cfg(test)]
mod test {
    use super::Header;

    #[test]
    fn header_bytes() {
        let expected = Header {
            kind: 20,
            size: 8 * 1024,
        };
        let exp_bytes = expected.as_bytes();

        let actual = Header::from_bytes(exp_bytes).unwrap();

        assert_eq!(expected, actual);
    }
}
