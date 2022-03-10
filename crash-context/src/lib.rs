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
#![allow(unsafe_code, nonstandard_style)]

use std::ffi::c_void;

cfg_if::cfg_if! {
    if #[cfg(any(
        target_os = "linux",
        target_os = "l4re",
        target_os = "android",
        target_os = "emscripten"))
    ] {
        #[repr(C)]
        #[derive(Copy, Clone)]
        pub struct sigset_t {
            #[cfg(target_pointer_width = "32")]
            __val: [u32; 32],
            #[cfg(target_pointer_width = "64")]
            __val: [u64; 16],
        }
    }
}

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")] {
        #[repr(C)]
        #[derive(Clone)]
        pub struct ucontext_t {
            pub uc_flags: u64,
            pub uc_link: *mut ucontext_t,
            pub uc_stack: stack_t,
            pub uc_mcontext: mcontext_t,
            pub uc_sigmask: sigset_t,
            __private: [u8; 512],
        }

        #[repr(C)]
        #[derive(Clone)]
        pub struct stack_t {
            pub ss_sp: *mut c_void,
            pub ss_flags: i32,
            pub ss_size: usize,
        }

        #[repr(C)]
        #[derive(Clone)]
        pub struct mcontext_t {
            pub gregs: [i64; 23],
            pub fpregs: *mut fpregset_t,
            __reserved: [u64; 8],
        }

        #[repr(C)]
        #[derive(Clone)]
        pub struct fpregset_t {
            pub cwd: u16,
            pub swd: u16,
            pub ftw: u16,
            pub fop: u16,
            pub rip: u64,
            pub rdp: u64,
            pub mxcsr: u32,
            pub mxcr_mask: u32,
            pub st_space: [u32; 32],
            pub xmm_space: [u32; 64],
            __padding: [u64; 12],
        }

        mod x86_64;
        pub use x86_64::getcontext;
    }
}

extern "C" {
    pub fn getcontext(uc: *mut ucontext_t) -> i32;
}

#[cfg(test)]
mod test {
    #[test]
    fn gets_context() {
        unsafe {
            let mut uctx = std::mem::zeroed();

            assert_eq!(super::getcontext(&mut uctx), 0);

            assert!(!uctx.uc_mcontext.fpregs.is_null());
        }
    }

    // Musl doesn't contain fpregs in libc because reasons https://github.com/rust-lang/libc/pull/1646
    #[cfg(not(target_env = "musl"))]
    #[test]
    fn matches_libc() {
        assert_eq!(
            std::mem::size_of::<libc::ucontext_t>(),
            std::mem::size_of::<super::ucontext_t>()
        );
    }
}
