#include "../async-registration/queued-host.c"
int32_t aggregate_async_cancel_complete(void) {
    if (seen_pending != 2 || seen_ready != 0 || result_drops != 1 ||
        drops != 1 || requests != 1 || polls != 2 || rejected != 4 ||
        pumped != 1 || allocations != 2 || frees != 2 || source.context || source.callback) return 70;
    puts("prepared aggregate cancelled; late entry rejected; no-future before frees");
    return 0;
}
