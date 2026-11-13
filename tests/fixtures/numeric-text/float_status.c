#define _GNU_SOURCE
#include <errno.h>
#include <fenv.h>
#include <locale.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

static const int modes[]={FE_TONEAREST,FE_DOWNWARD,FE_UPWARD,FE_TOWARDZERO};
static const int seeds[]={0,ERANGE,EINVAL};
static const char *const doubles[]={
 "1.7976931348623157e308","-1.7976931348623157e308","5e-324","-5e-324",
 "1.7976931348623155e308","-1.7976931348623155e308","1e-99999","-1e-99999","0","-0"};
static const char *const singles[]={
 "3.4028234663852885e38","-3.4028234663852885e38","1.5e-45","-1.5e-45",
 "3.4028232e38","-3.4028232e38","1e-99999","-1e-99999","0","-0"};
static uint64_t expected64[10];
static uint32_t expected32[10];
static int previous_mode,previous_errno,mode,seed,active;
static locale_t previous_locale,current_locale,c_locale;
int32_t probe_float_begin(int32_t rounding,int32_t pollution) {
 if(active || rounding<0 || rounding>3 || pollution<0 || pollution>2) return 0;
 previous_errno=errno;previous_mode=fegetround();previous_locale=uselocale((locale_t)0);
 c_locale=newlocale(LC_ALL_MASK,"C",(locale_t)0);
 current_locale=newlocale(LC_ALL_MASK,"C.UTF-8",(locale_t)0);
 if(!current_locale) current_locale=newlocale(LC_ALL_MASK,"C",(locale_t)0);
 if(!c_locale || !current_locale || !uselocale(current_locale) || fesetround(modes[rounding])) return 0;
 mode=modes[rounding];seed=seeds[pollution];
 for(unsigned i=0;i<10;i++) {
  double d=strtod_l(doubles[i],NULL,c_locale);float f=strtof_l(singles[i],NULL,c_locale);
  memcpy(&expected64[i],&d,sizeof d);memcpy(&expected32[i],&f,sizeof f);
 }
 active=1;errno=seed;return 1;
}
static int state(void) {return active && errno==seed && fegetround()==mode && uselocale((locale_t)0)==current_locale;}
int32_t probe_float64(double value,int32_t index) {
 uint64_t bits;memcpy(&bits,&value,sizeof bits);
 return state() && index>=0 && index<10 && bits==expected64[index];
}
int32_t probe_float32(float value,int32_t index) {
 uint32_t bits;memcpy(&bits,&value,sizeof bits);
 return state() && index>=0 && index<10 && bits==expected32[index];
}
int32_t probe_float_end(void) {
 int ok=state();
 if(!active) return 0;
 if(fesetround(previous_mode) || !uselocale(previous_locale)) ok=0;
 freelocale(current_locale);freelocale(c_locale);active=0;errno=previous_errno;
 return ok;
}
