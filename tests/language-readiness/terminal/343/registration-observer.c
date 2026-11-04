/* Strong same-thread backend, native callback client and allocator observer.
 * All entered callbacks (including rejected raw entries) return before close;
 * pointers are withdrawn before close returns. No ZIL runtime is implemented. */
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
typedef uint64_t (*Callback)(void *,int32_t);
static Callback callback;
static void *context,*allocation;
static int attempts,installs,closed,dropped,freed,admitted,reentry_rejected,late_rejected;
void *__real_calloc(size_t,size_t);
void __real_free(void *);
static void require(int yes,int line){if(!yes){fprintf(stderr,"registration observer failed line%d\n",line);abort();}}
#define REQUIRE(x) require((x),__LINE__)
void *__wrap_calloc(size_t n,size_t size){
    ++attempts;
    REQUIRE(attempts==1 && n==1 && size>=32);
    if(getenv("ZIL343_REG_ALLOC_FAIL"))return NULL;
    allocation=__real_calloc(n,size);
    REQUIRE(allocation!=NULL);
    return allocation;
}
void __wrap_free(void *p){
    REQUIRE(p==allocation && dropped==1 && freed==0);
    REQUIRE(closed==1 || getenv("ZIL343_REG_INSTALL_FAIL"));
    ++freed;allocation=NULL;__real_free(p);
}
void registration_note(int32_t code){REQUIRE(code==37 && dropped++==0);}
int32_t registration_reenter(void){
    ++admitted;
    REQUIRE(callback && context);
    uint64_t answer=callback(context,99);
    REQUIRE(answer==(UINT64_C(1)<<32));
    ++reentry_rejected;
    return 0;
}
void *registration_register(Callback cb,void *userdata){
    REQUIRE(installs++==0 && userdata==allocation && userdata!=NULL);
    callback=cb;context=userdata;
    REQUIRE(callback(context,1)==39);
    if(getenv("ZIL343_REG_INSTALL_FAIL")){
        callback=NULL;context=NULL; /* all accesses have ended before null */
        return NULL;
    }
    return userdata;
}
void registration_close_backend(void *token){
    REQUIRE(token==context && token==allocation && closed==0 && dropped==0);
    for(int i=0;i<2;++i){
        REQUIRE(callback(context,7)==(UINT64_C(1)<<32));
        ++late_rejected;
    }
    callback=NULL;context=NULL; /* strong no-future plus all raw entries drained */
    ++closed;
}
__attribute__((noinline)) int32_t registration_clobber(void){
    volatile unsigned char bytes[65536];
    for(size_t i=0;i<sizeof(bytes);++i)bytes[i]=(unsigned char)i;
    return (int32_t)bytes[65535]-255;
}
int32_t registration_fire(int32_t value){
    REQUIRE(callback && context && context==allocation);
    uint64_t answer=callback(context,value);
    REQUIRE((answer>>32)==0);
    return (int32_t)(uint32_t)answer;
}
static void final_check(void){
    if(!(attempts==1 && dropped==1 && allocation==NULL && callback==NULL && context==NULL)) fprintf(stderr,"registration counts attempts=%d installs=%d closed=%d dropped=%d freed=%d admitted=%d reentry=%d late=%d\n",attempts,installs,closed,dropped,freed,admitted,reentry_rejected,late_rejected);
    REQUIRE(attempts==1 && dropped==1 && allocation==NULL && callback==NULL && context==NULL);
    if(getenv("ZIL343_REG_ALLOC_FAIL")){REQUIRE(installs==0 && closed==0 && freed==0 && admitted==0);}
    else if(getenv("ZIL343_REG_INSTALL_FAIL")){REQUIRE(installs==1 && closed==0 && freed==1 && admitted==1 && reentry_rejected==1);}
    else {REQUIRE(installs==1 && closed==1 && freed==1 && admitted==2 && reentry_rejected==2 && late_rejected==2);}
}
__attribute__((constructor)) static void start_observer(void){REQUIRE(atexit(final_check)==0);}
