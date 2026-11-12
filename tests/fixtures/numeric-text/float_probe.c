#define _GNU_SOURCE
#include <locale.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static unsigned created, released, installs, restores;
static locale_t original;
int test_float_mode(void) {const char *text=getenv("ZIL_FLOAT_MODE");return text?atoi(text):0;}
locale_t __real_newlocale(int,const char *,locale_t);
locale_t __real_uselocale(locale_t);
void __real_freelocale(locale_t);
locale_t __wrap_newlocale(int mask,const char *name,locale_t base) {
    if(test_float_mode()==1) return (locale_t)0;
    locale_t value=__real_newlocale(mask,name,base);
    if(value) ++created;
    return value;
}
locale_t __wrap_uselocale(locale_t value) {
    if(value && value!=original && test_float_mode()==2) return (locale_t)0;
    locale_t previous=__real_uselocale(value);
    if(previous && value) {if(value==original) ++restores;else ++installs;}
    return previous;
}
void __wrap_freelocale(locale_t value) {++released;__real_freelocale(value);}
static uint64_t double_bits(int32_t index) {
    static const uint64_t edge[]={0,UINT64_C(0x8000000000000000),1,UINT64_C(0xfffffffffffff),UINT64_C(0x10000000000000),UINT64_C(0x7fefffffffffffff)};
    if(index<6) return edge[index];
    uint64_t bits=UINT64_C(0x9e3779b97f4a7c15)*(uint64_t)(index+1);
    bits^=bits>>29;bits*=UINT64_C(0xbf58476d1ce4e5b9);
    if((bits&UINT64_C(0x7ff0000000000000))==UINT64_C(0x7ff0000000000000)) bits^=UINT64_C(0x10000000000000);
    return bits;
}
static uint32_t float_bits(int32_t index) {
    static const uint32_t edge[]={0,UINT32_C(0x80000000),1,UINT32_C(0x7fffff),UINT32_C(0x800000),UINT32_C(0x7f7fffff)};
    if(index<6) return edge[index];
    uint32_t bits=UINT32_C(0x9e3779b9)*(uint32_t)(index+1);bits^=bits>>13;
    if((bits&UINT32_C(0x7f800000))==UINT32_C(0x7f800000)) bits^=UINT32_C(0x800000);
    return bits;
}
double input_f64(int32_t index) {uint64_t bits=double_bits(index);double value;memcpy(&value,&bits,sizeof value);return value;}
float input_f32(int32_t index) {uint32_t bits=float_bits(index);float value;memcpy(&value,&bits,sizeof value);return value;}
int32_t check_f64(double value,int32_t index) {uint64_t bits;memcpy(&bits,&value,sizeof bits);return bits==double_bits(index) && __real_uselocale((locale_t)0)==original;}
int32_t check_f32(float value,int32_t index) {uint32_t bits;memcpy(&bits,&value,sizeof bits);return bits==float_bits(index) && __real_uselocale((locale_t)0)==original;}
__attribute__((constructor)) static void start(void) {original=__real_uselocale((locale_t)0);}
__attribute__((destructor)) static void report(void) {
    fprintf(stderr,"locale created=%u released=%u installs=%u restores=%u\n",created,released,installs,restores);
    if(created!=released || installs!=restores || __real_uselocale((locale_t)0)!=original) _Exit(98);
}
