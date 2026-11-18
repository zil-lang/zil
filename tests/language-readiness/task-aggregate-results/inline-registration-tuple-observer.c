#include "../async-registration/queued-host.c"

int32_t tuple_close_complete(void) {
    if (seen_pending != 1 || seen_ready || result_drops || drops != 1 ||
        requests != 1 || polls != 2 || rejected != 4 || pumped != 1 ||
        allocations != 2 || frees != 2 || source.queued || source.context ||
        source.callback) return 70;
    puts("nested tuple: no-future before release");
    return 0;
}

int32_t tuple_interrupted_complete(void) {
    if (seen_pending != 2 || seen_ready || result_drops != 1 || drops != 1 ||
        requests != 1 || polls != 2 || rejected != 4 || pumped != 1 ||
        allocations != 2 || frees != 2 || source.queued || source.context ||
        source.callback) return 70;
    puts("nested tuple: cancelled after actual first drop Pending");
    return 0;
}

int32_t tuple_failure_complete(void) {
    if (seen_pending != 2 || seen_ready != 1 || result_drops || drops != 1 ||
        requests != 1 || polls != 2 || rejected != 4 || pumped != 1 ||
        allocations != 2 || frees != 2 || source.queued || source.context ||
        source.callback) return 72;
    puts("partial tuple: failure observed after no-future and both frees");
    return 0;
}
