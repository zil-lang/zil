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
int invoke_integer_callbacks(int8_t (*a)(int8_t), int8_t (*again)(int8_t),
    int16_t (*b)(int16_t), uint32_t (*c)(uint32_t), intptr_t (*d)(intptr_t)) {
    if(a!=again) return 1;
    if(a(INT8_MIN)!=INT8_MIN || a(-1)!=-1 || a(INT8_MAX)!=INT8_MAX) return 2;
    if(b(INT16_MIN)!=INT16_MIN || b(-1)!=-1 || b(INT16_MAX)!=INT16_MAX) return 3;
    if(c(UINT32_MAX)!=UINT32_MAX || c(UINT32_C(0x80000000))!=UINT32_C(0x80000000)) return 4;
    if(d(INTPTR_MIN)!=INTPTR_MIN || d(INTPTR_MAX)!=INTPTR_MAX) return 5;
    return 42;
}

#include <string.h>
struct WideNumbers {uint64_t a; size_t b; float c; double d;};
_Static_assert(sizeof(struct WideNumbers)==32,"wide size");
_Static_assert(offsetof(struct WideNumbers,c)==16,"float offset");
_Static_assert(offsetof(struct WideNumbers,d)==24,"double offset");
uint64_t echo_u64(uint64_t value) {return value;}
size_t echo_usize(size_t value) {return value;}
int inspect_wide(const struct WideNumbers *p) {
 return p->a==UINT64_MAX && p->b==SIZE_MAX && p->c==-1.25f && p->d==3.5 ? 0 : 1;
}
void mutate_wide(struct WideNumbers *p) {p->a=UINT64_C(0x8000000000000000);p->b=42;p->c=2.5f;p->d=-7.0;}
int invoke_remaining_callbacks(uint8_t (*a)(uint8_t),uint8_t (*again)(uint8_t),uint16_t (*b)(uint16_t),
 uint64_t (*c)(uint64_t),size_t (*d)(size_t),int32_t (*e)(int32_t),int64_t (*f)(int64_t),
 float (*g)(float),double (*h)(double)) {
 if(a!=again) return 1;
 for(unsigned i=0;i<256;i++) if(a((uint8_t)i)!=(uint8_t)i) return 2;
 if(b(UINT16_MAX)!=UINT16_MAX || b(0x8000)!=0x8000) return 3;
 if(c(UINT64_MAX)!=UINT64_MAX || c(UINT64_C(0x8000000000000000))!=UINT64_C(0x8000000000000000))return 4;
 if(d(SIZE_MAX)!=SIZE_MAX || e(INT32_MIN)!=INT32_MIN || f(INT64_MIN)!=INT64_MIN) return 5;
 if(g(-1.25f)!=-1.25f || h(3.5)!=3.5) return 6;
 uint32_t bits32=UINT32_C(0x7fc12345), got32; float nan32;memcpy(&nan32,&bits32,4);float out32=g(nan32);memcpy(&got32,&out32,4);
 uint64_t bits64=UINT64_C(0x7ff8123456789abc),got64;double nan64;memcpy(&nan64,&bits64,8);double out64=h(nan64);memcpy(&got64,&out64,8);
 return got32==bits32 && got64==bits64 ? 42 : 7;
}
