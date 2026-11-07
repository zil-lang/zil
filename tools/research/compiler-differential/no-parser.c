
/* Test-only trap: never part of ZIL codegen. */
#include <unistd.h>
int LLVMParseIRInContext(void *context,void *buffer,void **module,char **error) {
    (void)context;(void)buffer;(void)module;(void)error;
    static const char message[]="ZIL210_PARSEIR_FORBIDDEN\n";
    write(2,message,sizeof(message)-1);
    _exit(86);
}
