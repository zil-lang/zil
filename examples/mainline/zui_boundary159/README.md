# ZUI boundary159 consumer
Three ordinary source modules: four frames / 16384 FIFO events, importing a public nominal trait and mutable method, invoking those methods in an owned FnMut, and resetting its owned ByteBuffer via plain local assignment. Every frame checks every output byte; queue closes empty and releases storage, while the draw callback resets before its own destruction.
This is a headless core readiness test, not a window/GPU application, retained UI graph, generic owning container, nested-owning callback support or cross-thread shutdown proof.
