#include <stdint.h>
#include <stddef.h>
struct Numbers { int8_t a; int16_t b; uint32_t c; intptr_t d; };
_Static_assert(sizeof(struct Numbers)==16,"size");
_Static_assert(_Alignof(struct Numbers)==8,"align");
_Static_assert(offsetof(struct Numbers,a)==0,"i8");
_Static_assert(offsetof(struct Numbers,b)==2,"i16");
_Static_assert(offsetof(struct Numbers,c)==4,"u32");
_Static_assert(offsetof(struct Numbers,d)==8,"isize");
int8_t echo_signed_byte(int8_t value) { return value; }
int16_t echo_signed_word(int16_t value) { return value; }
uint32_t echo_unsigned_word(uint32_t value) { return value; }
intptr_t echo_native_word(intptr_t value) { return value; }
int inspect_numbers(const struct Numbers *p) {
 return p->a==INT8_MIN && p->b==INT16_MIN && p->c==UINT32_MAX && p->d==INTPTR_MIN ? 0 : 1;
}
void mutate_numbers(struct Numbers *p) {p->a=INT8_MAX;p->b=INT16_MAX;p->c=UINT32_C(0x80000000);p->d=INTPTR_MAX;}
