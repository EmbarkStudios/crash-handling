// Copyright 2015 The Crashpad Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[derive(Copy, Clone)]
#[repr(C, align(16))]
pub struct M128A {
    pub Low: u64,
    pub High: i64,
}

#[derive(Copy, Clone)]
#[repr(C, align(16))]
pub struct XSAVE_FORMAT {
    pub ControlWord: u16,
    pub StatusWord: u16,
    pub TagWord: u8,
    pub Reserved1: u8,
    pub ErrorOpcode: u16,
    pub ErrorOffset: u32,
    pub ErrorSelector: u16,
    pub Reserved2: u16,
    pub DataOffset: u32,
    pub DataSelector: u16,
    pub Reserved3: u16,
    pub MxCsr: u32,
    pub MxCsr_Mask: u32,
    pub FloatRegisters: [M128A; 8],
    pub XmmRegisters: [M128A; 16],
    pub Reserved4: [u8; 96],
}

#[derive(Copy, Clone)]
#[repr(C, align(16))]
pub struct CONTEXT {
    pub P1Home: u64,
    pub P2Home: u64,
    pub P3Home: u64,
    pub P4Home: u64,
    pub P5Home: u64,
    pub P6Home: u64,
    pub ContextFlags: u32,
    pub MxCsr: u32,
    pub SegCs: u16,
    pub SegDs: u16,
    pub SegEs: u16,
    pub SegFs: u16,
    pub SegGs: u16,
    pub SegSs: u16,
    pub EFlags: u32,
    pub Dr0: u64,
    pub Dr1: u64,
    pub Dr2: u64,
    pub Dr3: u64,
    pub Dr6: u64,
    pub Dr7: u64,
    pub Rax: u64,
    pub Rcx: u64,
    pub Rdx: u64,
    pub Rbx: u64,
    pub Rsp: u64,
    pub Rbp: u64,
    pub Rsi: u64,
    pub Rdi: u64,
    pub R8: u64,
    pub R9: u64,
    pub R10: u64,
    pub R11: u64,
    pub R12: u64,
    pub R13: u64,
    pub R14: u64,
    pub R15: u64,
    pub Rip: u64,
    pub Anonymous: CONTEXT_0,
    pub VectorRegister: [M128A; 26],
    pub VectorControl: u64,
    pub DebugControl: u64,
    pub LastBranchToRip: u64,
    pub LastBranchFromRip: u64,
    pub LastExceptionToRip: u64,
    pub LastExceptionFromRip: u64,
}

#[derive(Copy, Clone)]
#[repr(C, align(16))]
pub union CONTEXT_0 {
    pub FltSave: XSAVE_FORMAT,
    pub Anonymous: CONTEXT_0_0,
}

#[derive(Copy, Clone)]
#[repr(C, align(16))]
pub struct CONTEXT_0_0 {
    pub Header: [M128A; 2],
    pub Legacy: [M128A; 8],
    pub Xmm0: M128A,
    pub Xmm1: M128A,
    pub Xmm2: M128A,
    pub Xmm3: M128A,
    pub Xmm4: M128A,
    pub Xmm5: M128A,
    pub Xmm6: M128A,
    pub Xmm7: M128A,
    pub Xmm8: M128A,
    pub Xmm9: M128A,
    pub Xmm10: M128A,
    pub Xmm11: M128A,
    pub Xmm12: M128A,
    pub Xmm13: M128A,
    pub Xmm14: M128A,
    pub Xmm15: M128A,
}

std::arch::global_asm! {
  ".text",
  ".global capture_context",
"capture_context:",
  ".seh_proc capture_context",
  "push %rbp",
  ".seh_pushreg %rbp",
  "movq %rsp, %rbp",
  ".seh_setframe %rbp, 0",

  // Note that 16-byte stack alignment is not maintained because this function
  // does not call out to any other.

  // pushfq first, because some instructions affect rflags. rflags will be in [rbp-8].
  "pushfq",
  ".seh_stackalloc 8",
  ".seh_endprologue",

  "movl $0x10000f, 0x30(%rcx)",  // ContextFlags

  // General-purpose registers whose values haven’t changed can be captured directly.
  "movq %rax, 0x78(%rcx)",       // Rax
  "movq %rdx, 0x88(%rcx)",       // Rdx
  "movq %rbx, 0x90(%rcx)",       // Rbx
  "movq %rsi, 0xa8(%rcx)",       // Rsi
  "movq %rdi, 0xb0(%rcx)",       // Rdi
  "movq %r8, 0xb8(%rcx)",        // R8
  "movq %r9, 0xc0(%rcx)",        // R9
  "movq %r10, 0xc8(%rcx)",       // R10
  "movq %r11, 0xd0(%rcx)",       // R11
  "movq %r12, 0xd8(%rcx)",       // R12
  "movq %r13, 0xe0(%rcx)",       // R13
  "movq %r14, 0xe8(%rcx)",       // R14
  "movq %r15, 0xf0(%rcx)",       // R15

  // Because of the calling convention, there’s no way to recover the value of
  // the caller’s rcx as it existed prior to calling this function. This
  // function captures a snapshot of the register state at its return, which
  // involves rcx containing a pointer to its first argument.
  "movq %rcx, 0x80(%rcx)",       // Rcx

  // Now that the original value of rax has been saved, it can be repurposed to
  // hold other registers’ values.

  // Save mxcsr. This is duplicated in context->FltSave.MxCsr, saved by fxsave
  // below.
  "stmxcsr 0x34(%rcx)",         // MxCsr

  // Segment registers.
  "movw %cs, 0x38(%rcx)",        // SegCs
  "movw %ds, 0x3a(%rcx)",        // SegDs
  "movw %es, 0x3c(%rcx)",        // SegEs
  "movw %fs, 0x3e(%rcx)",        // SegFs
  "movw %gs, 0x40(%rcx)",        // SegGs
  "movw %ss, 0x42(%rcx)",        // SegSs

  // The original rflags was saved on the stack above. Note that the CONTEXT
  // structure only stores eflags, the low 32 bits. The high 32 bits in rflags
  // are reserved.
  "movq -8(%rbp), %rax",
  "mov %eax, 0x44(%rcx)",       // EFlags

  // rsp was saved in rbp in this function’s prologue, but the caller’s rsp is
  // 16 more than this value: 8 for the original rbp saved on the stack in this
  // function’s prologue, and 8 for the return address saved on the stack by the
  // call instruction that reached this function.
  "leaq 16(%rbp), %rax",
  "movq %rax, 0x98(%rcx)",

  // The original rbp was saved on the stack in this function’s prologue.
  "movq %rbp, %rax",
  "movq %rax, 0xa0(%rcx)",

  // rip can’t be accessed directly, but the return address saved on the stack by
  // the call instruction that reached this function can be used.
  "movq 8(%rbp), %rax",
  "movq %rax, 0xf8(%rcx)",

  // Zero out the fxsave area before performing the fxsave. Some of the fxsave
  // area may not be written by fxsave, and some is definitely not written by
  // fxsave. This also zeroes out the rest of the CONTEXT structure to its end,
  // including the unused VectorRegister and VectorControl fields, and the debug
  // control register fields.
  "movq %rcx, %rbx",
  "cld",
  "leaq 0x100(%rcx), %rdi",
  "xor %rax, %rax",
  "movq $122, %rcx",
  "rep stosq",
  "movq %rbx, %rcx",

  // Save the floating point (including SSE) state. The CONTEXT structure is
  // declared as 16-byte-aligned, which is correct for this operation.
  "fxsave 0x100(%rcx)",

  // TODO: AVX/xsave support. https://crashpad.chromium.org/bug/58

  // The register parameter home address fields aren’t used, so zero them out.
  "movq $0, 0x0(%rcx)",
  "movq $0, 0x8(%rcx)",
  "movq $0, 0x10(%rcx)",
  "movq $0, 0x18(%rcx)",
  "movq $0, 0x20(%rcx)",

  // The debug registers can’t be read from user code, so zero them out in the
  // CONTEXT structure. context->ContextFlags doesn’t indicate that they are
  // present.
  "movq $0, 0x48(%rcx)",
  "movq $0, 0x50(%rcx)",
  "movq $0, 0x58(%rcx)",
  "movq $0, 0x60(%rcx)",
  "movq $0, 0x68(%rcx)",
  "movq $0, 0x70(%rcx)",
  "movq $0, 0x78(%rcx)",

  // Clean up by restoring clobbered registers, even those considered volatile by
  // the ABI, so that the captured context represents the state at this
  // function’s exit.
  "movq 0x78(%rcx), %rax",
  "movq 0x90(%rcx), %rbx",
  "movq 0xb0(%rcx), %rdi",
  "popfq",

  "popq %rbp",
  "ret",
  ".seh_endproc",
  options(att_syntax)
}
