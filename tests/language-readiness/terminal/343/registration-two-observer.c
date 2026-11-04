/* Two actual retained environments; same-thread strong backend and observations. */
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
typedef uint64_t (*Callback)(void *,int32_t);
struct Record {void *allocation,*context;Callback callback;int installed,closed,dropped,freed,admitted,busy,late;};
static struct Record records[2];
static int allocations,active=-1;
void *__real_calloc(size_t,size_t);void __real_free(void *);
static void require(int yes,int line){if(!yes){fprintf(stderr,"two registrations failed line%d\n",line);abort();}}
#define REQUIRE(x) require((x),__LINE__)
void *__wrap_calloc(size_t n,size_t size){
    REQUIRE(allocations<2 && n==1 && size>=32);
    void *p=__real_calloc(n,size);REQUIRE(p!=NULL);
    records[allocations++].allocation=p;
    REQUIRE(allocations!=2 || records[0].allocation!=records[1].allocation);
    return p;
}
void __wrap_free(void *p){
    int i=records[0].allocation==p?0:1;struct Record *r=&records[i];
    REQUIRE(r->allocation==p && r->closed==1 && r->dropped==1 && r->freed++==0);
    r->allocation=NULL;__real_free(p);
}
void registration_note(int32_t code){int i=code==37?0:1;REQUIRE(code==37+10*i && records[i].dropped++==0);}
int32_t registration_reenter(void){
    REQUIRE(active>=0 && active<2);struct Record *r=&records[active];
    ++r->admitted;REQUIRE(r->callback(r->context,99)==(UINT64_C(1)<<32));++r->busy;return 0;
}
static uint64_t invoke(int i,int32_t value){int previous=active;active=i;struct Record *r=&records[i];uint64_t result=r->callback(r->context,value);active=previous;return result;}
void *registration_register(Callback cb,void *userdata){
    int i=records[0].allocation==userdata?0:1;struct Record *r=&records[i];
    REQUIRE(r->allocation==userdata && r->installed++==0);r->callback=cb;r->context=userdata;
    REQUIRE(invoke(i,1)==(uint64_t)(39+10*i));return userdata;
}
void registration_close_backend(void *token){
    int i=records[0].context==token?0:1;struct Record *r=&records[i];
    REQUIRE(r->context==token && r->allocation==token && r->closed==0 && r->dropped==0);
    for(int j=0;j<2;++j){REQUIRE(invoke(i,7)==(UINT64_C(1)<<32));++r->late;}
    r->callback=NULL;r->context=NULL;++r->closed; /* all raw entries ended; no future access */
}
__attribute__((noinline)) int32_t registration_clobber(void){volatile unsigned char bytes[65536];for(size_t i=0;i<sizeof(bytes);++i)bytes[i]=(unsigned char)i;return (int32_t)bytes[65535]-255;}
int32_t registration_fire_at(int32_t i,int32_t value){REQUIRE(i>=0 && i<2 && records[i].callback && records[i].context==records[i].allocation);uint64_t result=invoke(i,value);REQUIRE((result>>32)==0);return (int32_t)(uint32_t)result;}
static void final_check(void){REQUIRE(allocations==2);for(int i=0;i<2;++i){struct Record *r=&records[i];REQUIRE(!r->allocation && !r->context && !r->callback && r->installed==1 && r->closed==1 && r->dropped==1 && r->freed==1 && r->admitted==3 && r->busy==3 && r->late==2);}}
__attribute__((constructor)) static void start_observer(void){REQUIRE(atexit(final_check)==0);}
