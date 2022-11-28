const SIZE_OF_80387_REGISTERS: usize = 80;
const MAXIMUM_SUPPORTED_EXTENSION: usize = 512;

#[derive(Copy, Clone)]
#[repr(C, align(16))]
pub struct FLOATING_SAVE_AREA {
    ControlWord: u32,
    StatusWord: u32,
    TagWord: u32,
    ErrorOffset: u32,
    ErrorSelector: u32,
    DataOffset: u32,
    DataSelector: u32,
    RegisterArea: [u8; SIZE_OF_80387_REGISTERS],
    Spare0: u32,
}

#[derive(Copy, Clone)]
#[repr(C, align(16))]
pub struct CONTEXT {
    pub ContextFlags: u32,
    pub Dr0: u32,
    pub Dr1: u32,
    pub Dr2: u32,
    pub Dr3: u32,
    pub Dr6: u32,
    pub Dr7: u32,
    pub FloatSave: FLOATING_SAVE_AREA,
    pub SegGs: u32,
    pub SegFs: u32,
    pub SegEs: u32,
    pub SegDs: u32,
    pub Edi: u32,
    pub Esi: u32,
    pub Ebx: u32,
    pub Edx: u32,
    pub Ecx: u32,
    pub Eax: u32,
    pub Ebp: u32,
    pub Eip: u32,
    pub SegCs: u32,
    pub EFlags: u32,
    pub Esp: u32,
    pub SegSs: u32,
    pub ExtendedRegisters: [u8; MAXIMUM_SUPPORTED_EXTENSION],
}

std::arch::global_asm! {
  ".text",
  ".global capture_context",
"capture_context:",
  "push ebp",
  "mov ebp, esp",

  // pushfd first, because some instructions affect eflags. eflags will be in
  // [ebp-4].
  "pushfd",

  // Save the original value of ebx, and use ebx to hold the CONTEXT* argument.
  // The original value of ebx will be in [ebp-8].
  "push ebx",
  "mov ebx, [ebp+8]",

  // General-purpose registers whose values haven’t changed can be captured
  // directly.
  "mov dword ptr [ebx+0x9c], edi",
  "mov dword ptr [ebx+0xa0], esi",
  "mov dword ptr [ebx+0xa8], edx",
  "mov dword ptr [ebx+0xac], ecx",
  "mov dword ptr [ebx+0xb0], eax",

  // Now that the original value of edx has been saved, it can be repurposed to
  // hold other registers’ values.

  // The original ebx was saved on the stack above.
  "mov edx, dword ptr [ebp-8]",
  "mov [ebx+0xa4], edx",

  // The original ebp was saved on the stack in this function’s prologue.
  "mov edx, dword ptr [ebp]",
  "mov [ebx+0xb4], edx",

  // eip can’t be accessed directly, but the return address saved on the stack
  // by the call instruction that reached this function can be used.
  "mov edx, dword ptr [ebp+4]",
  "mov [ebx+0xb8], edx",

  // The original eflags was saved on the stack above.
  "mov edx, dword ptr [ebp-4]",
  "mov [ebx+0xc0], edx",

  // esp was saved in ebp in this function’s prologue, but the caller’s esp is 8
  // more than this value: 4 for the original ebp saved on the stack in this
  // function’s prologue, and 4 for the return address saved on the stack by the
  // call instruction that reached this function.
  "lea edx, [ebp+8]",
  "mov [ebx+0xc4], edx",

  // The segment registers are 16 bits wide, but CONTEXT declares them as
  // unsigned 32-bit values, so zero the top half.
  "xor edx, edx",
  "mov dx, gs",
  "mov [ebx+0xc8], edx",
  "mov dx, fs",
  "mov [ebx+0x90], edx",
  "mov dx, es",
  "mov [ebx+0x94], edx",
  "mov dx, ds",
  "mov [ebx+0x98], edx",
  "mov dx, cs",
  "mov [ebx+0xbc], edx",
  "mov dx, ss",
  "mov [ebx+0xc8], edx",

  // Prepare for the string move that will populate the ExtendedRegisters area,
  // or the string store that will zero it.
  "cld",

  // We assume fxsave is supported. It was introduced with the late iterations
  // of the Pentium II so we're pretty sure we'll always have it available.
  // Set ContextFlags to CONTEXT_i386 | CONTEXT_EXTENDED_REGISTERS |
  // CONTEXT_CONTROL | CONTEXT_INTEGER | CONTEXT_SEGMENTS |
  // CONTEXT_FLOATING_POINT | CONTEXT_EXTENDED_REGISTERS.
  "mov dword ptr [ebx], 0x0001003f",

  // fxsave requires a 16 byte-aligned destination memory area. Nothing
  // guarantees the alignment of a CONTEXT structure, so create a temporary
  // aligned fxsave destination on the stack.
  "and esp, 0xfffffff0",
  "sub esp, 512", // MAXIMUM_SUPPORTED_EXTENSION

  // Zero out the temporary fxsave area before performing the fxsave. Some of the
  // fxsave area may not be written by fxsave, and some is definitely not written
  // by fxsave.
  "mov edi, esp",
  "xor eax, eax",
  "mov ecx, 128", // MAXIMUM_SUPPORTED_EXTENSION / sizeof(DWORD)
  "rep stosd",

  "fxsave [esp]",

  // Copy the temporary fxsave area into the CONTEXT structure.
  "lea edi, [ebx+0xcc]",
  "mov esi, esp",
  "mov ecx, 128", // MAXIMUM_SUPPORTED_EXTENSION / sizeof(DWORD)
  "rep movsd",

  // Free the stack space used for the temporary fxsave area.
  "lea esp, [ebp-8]",

  // TODO: AVX/xsave support should be added here

  // fnsave reinitializes the FPU with an implicit finit operation, so use frstor
  // to restore the original state.
  "fnsave [ebx+0x1c]",
  "frstor [ebx+0x1c]",

  // cr0 is inaccessible from user code, and this field would not be used anyway.
  "mov dword ptr [ebx+0x88], 0",

  // The debug registers can’t be read from user code, so zero them out in the
  // CONTEXT structure. context->ContextFlags doesn’t indicate that they are
  // present.
  "mov dword ptr [ebx+0x04], 0",
  "mov dword ptr [ebx+0x08], 0",
  "mov dword ptr [ebx+0x0c], 0",
  "mov dword ptr [ebx+0x10], 0",
  "mov dword ptr [ebx+0x14], 0",
  "mov dword ptr [ebx+0x18], 0",

  // Clean up by restoring clobbered registers, even those considered volatile
  // by the ABI, so that the captured context represents the state at this
  // function’s exit.
  "mov edi, [ebx+0x9c]",
  "mov esi, [ebx+0xa0]",
  "mov edx, [ebx+0xa8]",
  "mov ecx, [ebx+0xac]",
  "mov eax, [ebx+0xb0]",
  "pop ebx",
  "popfd",

  "pop ebp",

  "ret",
}
