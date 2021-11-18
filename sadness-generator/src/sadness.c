#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

uint32_t definitely_not_zero() {
    return 0;
}

void sig_fpe() {
    uint32_t ohno = 1 / definitely_not_zero();
    printf("%u\n", ohno);
}

void sig_segv() {
    const uint32_t* oops = NULL;
    printf("%u\n", *oops);
}

void sig_ill() {
    // TODO: x86/_64 only
    asm("ud2");
}

// https://en.wikipedia.org/wiki/Bus_error
void sig_bus() {
    int *iptr;
    char *cptr;

    // Enable Alignment Checking on x86_64
    asm("pushf\norl $0x40000,(%rsp)\npopf");

    // malloc() always provides memory which is aligned for all fundamental types
    cptr = malloc(sizeof(int) + 1);
    
    // Increment the pointer by one, making it misaligned
    iptr = (int *) ++cptr;

    // Dereference it as an int pointer, causing an unaligned access
    *iptr = 42;
}

void sig_trap() {
    asm("int3");
}
