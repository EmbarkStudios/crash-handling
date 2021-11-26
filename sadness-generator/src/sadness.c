#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

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

void sig_bus(const char* path, size_t path_len) {
    #if 0
        // https://en.wikipedia.org/wiki/Bus_error
        int *iptr;
        char *cptr;

        // This address alignment bus error might actually be a kind we can't
        // effectively handle, as we need to change processor flags, which
        // actually affects the entire process, meaning we can crash inside the
        // signal handler just doing something simple like zeroing memory. This
        // is probably fine not to handle, at least on x86_64 because it is not
        // an kind of SIGBUS that should really happen in practice

        // Enable Alignment Checking on x86_64
        asm("pushf\norl $0x40000,(%rsp)\npopf");

        // malloc() always provides memory which is aligned for all fundamental types
        cptr = malloc(sizeof(int) + 1);
        
        // Increment the pointer by one, making it misaligned
        iptr = (int *) ++cptr;

        // Dereference it as an int pointer, causing an unaligned access
        *iptr = 42;
    #else
        char* fpath = calloc(path_len + 1, 1);
        strncpy(fpath, path, path_len);
        int bus_fd = open(fpath, O_RDWR | O_CREAT, 0666);
        uint8_t* bus_map = mmap(0, 128, PROT_READ | PROT_WRITE, MAP_SHARED, bus_fd, 0);

        printf("%u", bus_map[1]);

        // We won't get here, but it's best to be tidy
        free(fpath);
    #endif
}

void sig_trap() {
    asm("int3");
}
