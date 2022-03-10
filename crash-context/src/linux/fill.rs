use minidump_common::format;

#[cfg(target_arch = "x86_64")]
impl crate::CpuContext for super::CrashContext {
    fn instruction_pointer(&self) -> usize {
        self.context.uc_mcontext.gregs[libc::REG_RIP as usize] as usize
    }

    fn stack_pointer(&self) -> usize {
        self.context.uc_mcontext.gregs[libc::REG_RSP as usize] as usize
    }

    fn fill_cpu_context(&self, out: &mut crate::RawCpuContext) {
        use libc::{
            REG_CSGSFS, REG_EFL, REG_R10, REG_R11, REG_R12, REG_R13, REG_R14, REG_R15, REG_R8,
            REG_R9, REG_RAX, REG_RBP, REG_RBX, REG_RCX, REG_RDI, REG_RDX, REG_RIP, REG_RSI,
            REG_RSP,
        };

        out.context_flags = format::ContextFlagsAmd64::CONTEXT_AMD64_FULL.bits();

        {
            let gregs = &self.context.uc_mcontext.gregs;
            out.cs = (gregs[REG_CSGSFS as usize] & 0xffff) as u16;

            out.fs = ((gregs[REG_CSGSFS as usize] >> 32) & 0xffff) as u16;
            out.gs = ((gregs[REG_CSGSFS as usize] >> 16) & 0xffff) as u16;

            out.eflags = gregs[REG_EFL as usize] as u32;

            out.rax = gregs[REG_RAX as usize] as u64;
            out.rcx = gregs[REG_RCX as usize] as u64;
            out.rdx = gregs[REG_RDX as usize] as u64;
            out.rbx = gregs[REG_RBX as usize] as u64;

            out.rsp = gregs[REG_RSP as usize] as u64;
            out.rbp = gregs[REG_RBP as usize] as u64;
            out.rsi = gregs[REG_RSI as usize] as u64;
            out.rdi = gregs[REG_RDI as usize] as u64;
            out.r8 = gregs[REG_R8 as usize] as u64;
            out.r9 = gregs[REG_R9 as usize] as u64;
            out.r10 = gregs[REG_R10 as usize] as u64;
            out.r11 = gregs[REG_R11 as usize] as u64;
            out.r12 = gregs[REG_R12 as usize] as u64;
            out.r13 = gregs[REG_R13 as usize] as u64;
            out.r14 = gregs[REG_R14 as usize] as u64;
            out.r15 = gregs[REG_R15 as usize] as u64;

            out.rip = gregs[REG_RIP as usize] as u64;
        }

        {
            let fs = &self.float_state;

            let mut float_save = format::XMM_SAVE_AREA32 {
                control_word: fs.cwd,
                status_word: fs.swd,
                tag_word: fs.ftw as u8,
                error_opcode: fs.fop,
                error_offset: fs.rip as u32,
                data_offset: fs.rdp as u32,
                error_selector: 0, // We don't have this.
                data_selector: 0,  // We don't have this.
                mx_csr: fs.mxcsr,
                mx_csr_mask: fs.mxcr_mask,
                ..Default::default()
            };

            #[inline]
            pub fn to_u128(slice: &[u32]) -> &[u128] {
                unsafe {
                    std::slice::from_raw_parts(slice.as_ptr().cast(), slice.len().saturating_div(4))
                }
            }

            #[inline]
            pub fn copy_registers(dst: &mut [u128], src: &[u128]) {
                let to_copy = std::cmp::min(dst.len(), src.len());
                dst[..to_copy].copy_from_slice(&src[..to_copy]);
            }

            #[inline]
            pub fn copy_u32_registers(dst: &mut [u128], src: &[u32]) {
                copy_registers(dst, to_u128(src));
            }

            copy_u32_registers(&mut float_save.float_registers, &fs.st_space);
            copy_u32_registers(&mut float_save.xmm_registers, &fs.xmm_space);

            use scroll::Pwrite;
            out.float_save
                .pwrite_with(float_save, 0, scroll::Endian::Little)
                .expect("this is impossible");
        }
    }
}

#[cfg(target_arch = "x86")]
impl crate::CpuContext for super::CrashContext {
    fn instruction_pointer(&self) -> usize {
        self.context.uc_mcontext.gregs[libc::REG_EIP as usize] as usize
    }

    fn stack_pointer(&self) -> usize {
        self.context.uc_mcontext.gregs[libc::REG_ESP as usize] as usize
    }

    fn fill_cpu_context(&self, out: &mut crate::RawCpuContext) {
        use libc::{
            REG_CS, REG_DS, REG_EAX, REG_EBP, REG_EBX, REG_ECX, REG_EDI, REG_EDX, REG_EFL, REG_EIP,
            REG_ES, REG_ESI, REG_FS, REG_GS, REG_SS, REG_UESP,
        };

        out.context_flags = format::ContextFlagsX86::CONTEXT_X86_FULL.bits()
            | format::ContextFlagsX86::CONTEXT_X86_FLOATING_POINT.bits();

        {
            let gregs = &self.context.uc_mcontext.gregs;
            out.gs = gregs[REG_GS as usize] as u32;
            out.fs = gregs[REG_FS as usize] as u32;
            out.es = gregs[REG_ES as usize] as u32;
            out.ds = gregs[REG_DS as usize] as u32;

            out.edi = gregs[REG_EDI as usize] as u32;
            out.esi = gregs[REG_ESI as usize] as u32;
            out.ebx = gregs[REG_EBX as usize] as u32;
            out.edx = gregs[REG_EDX as usize] as u32;
            out.ecx = gregs[REG_ECX as usize] as u32;
            out.eax = gregs[REG_EAX as usize] as u32;

            out.ebp = gregs[REG_EBP as usize] as u32;
            out.eip = gregs[REG_EIP as usize] as u32;
            out.cs = gregs[REG_CS as usize] as u32;
            out.eflags = gregs[REG_EFL as usize] as u32;
            out.esp = gregs[REG_UESP as usize] as u32;
            out.ss = gregs[REG_SS as usize] as u32;
        }

        {
            let fs = &self.float_state;
            let mut out = &mut out.float_save;
            out.control_word = fs.cw;
            out.status_word = fs.sw;
            out.tag_word = fs.tag;
            out.error_offset = fs.ipoff;
            out.error_selector = fs.cssel;
            out.data_offset = fs.dataoff;
            out.data_selector = fs.datasel;

            debug_assert_eq!(fs._st.len() * std::mem::size_of::<super::fpreg_t>(), 80);
            out.register_area.copy_from_slice(unsafe {
                std::slice::from_raw_parts(
                    fs._st.as_ptr().cast(),
                    fs._st.len() * std::mem::size_of::<super::fpreg_t>(),
                )
            });
        }
    }
}
