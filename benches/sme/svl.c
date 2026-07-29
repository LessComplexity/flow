#include <stdio.h>
// Streaming vector length in bytes: rdsvl reads it architecturally.
__attribute__((target("sme")))
static unsigned long svl_bytes(void) {
    unsigned long v;
    __asm__ volatile("rdsvl %0, #1" : "=r"(v));
    return v;
}
int main(void) {
    unsigned long b = svl_bytes();
    printf("SVL = %lu bytes (%lu bits)\n", b, b * 8);
    printf("ZA  = %lu x %lu bytes\n", b, b);
    printf("one f32 ZA tile = %lu x %lu\n", b / 4, b / 4);
    printf("f32 ZA tiles available = 4, f64 = 8\n");
    return 0;
}
