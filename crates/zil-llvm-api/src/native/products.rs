//! Audited process-local C++ snapshot boundary. Only immutable byte products
//! can cross worker threads; live requests and all LLVM owners stay on one thread.
#[cfg(feature = "native-products")]
mod enabled {
    use super::super::{Handle, c_char, c_int, c_uint, ptr};
    use crate::{NativeProductStats, NativeWorkProducts};
    use std::sync::Arc;

    #[repr(C)]
    #[derive(Default)]
    struct WireStats {
        available: u64,
        optimizer_function_stage_executions: u64,
        optimizer_function_stage_reuses: u64,
        optimizer_module_pass_executions: u64,
        optimizer_cgscc_pass_executions: u64,
        analysis_reconstructions: u64,
        ir_key_bytes: u64,
        ir_snapshot_bytes: u64,
        ir_replay_bytes: u64,
        machine_function_executions: u64,
        machine_function_reuses: u64,
        machine_pass_skips: u64,
        asm_printer_runs: u64,
        mir_key_bytes: u64,
        mir_snapshot_bytes: u64,
        mir_replay_bytes: u64,
        retained_bytes: u64,
        failed_replays: u64,
    }
    impl From<WireStats> for NativeProductStats {
        fn from(value: WireStats) -> Self {
            Self {
                available: value.available,
                optimizer_function_stage_executions: value.optimizer_function_stage_executions,
                optimizer_function_stage_reuses: value.optimizer_function_stage_reuses,
                optimizer_module_pass_executions: value.optimizer_module_pass_executions,
                optimizer_cgscc_pass_executions: value.optimizer_cgscc_pass_executions,
                analysis_reconstructions: value.analysis_reconstructions,
                ir_key_bytes: value.ir_key_bytes,
                ir_snapshot_bytes: value.ir_snapshot_bytes,
                ir_replay_bytes: value.ir_replay_bytes,
                machine_function_executions: value.machine_function_executions,
                machine_function_reuses: value.machine_function_reuses,
                machine_pass_skips: value.machine_pass_skips,
                asm_printer_runs: value.asm_printer_runs,
                mir_key_bytes: value.mir_key_bytes,
                mir_snapshot_bytes: value.mir_snapshot_bytes,
                mir_replay_bytes: value.mir_replay_bytes,
                retained_bytes: value.retained_bytes,
                failed_replays: value.failed_replays,
                ..Self::default()
            }
        }
    }
    unsafe extern "C" {
        fn ZilNativeProductsCreateRequest(
            module: Handle,
            machine: Handle,
            prior: *const std::ffi::c_void,
            config: *const c_char,
            config_len: usize,
        ) -> Handle;
        fn ZilNativeProductsSetPhase(request: Handle, phase: c_uint);
        fn ZilNativeProductsFailed(request: *const std::ffi::c_void) -> c_int;
        fn ZilNativeProductsFinish(request: Handle, stats: *mut WireStats) -> Handle;
        fn ZilNativeProductsAbort(request: Handle, stats: *mut WireStats);
        fn ZilNativeProductsDispose(products: Handle);
    }

    #[derive(Debug)]
    pub(crate) struct Product(Handle);
    impl PartialEq for Product {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl Eq for Product {}
    // SAFETY: Finish returns immutable, independently owned IR/MIR bytes, fixed-DSO
    // pass keys and an opaque DSO-owned X86MachineFunctionInfo value copy. The pinned
    // clone helper admits only the closed x86-64/Linux input domain: StackPtrSaveMI
    // is null and PreallocatedIds is empty because its only writer's entire input
    // path is excluded. Remaining registers/frame indexes, scalars and value-only
    // containers are deeply copied; no LLVM Module/Context/Value/MachineInstr,
    // analysis result, MachineFunction or request/TLS owner crosses requests.
    // Restore reads this snapshot into a *current* MachineFunction; concurrent prior
    // readers do not mutate it. Arc keeps the independently owned snapshot alive
    // through every reader and the final DSO disposer releases it exactly once.
    unsafe impl Send for Product {}
    unsafe impl Sync for Product {}
    impl Drop for Product {
        fn drop(&mut self) {
            // SAFETY: the final Arc uniquely releases the completed native product.
            unsafe { ZilNativeProductsDispose(self.0) }
        }
    }

    pub(crate) struct Scope<'a> {
        raw: Handle,
        stats: &'a mut NativeProductStats,
        failed: &'a mut bool,
        // This borrow binds the prior's lifetime through Finish/Abort. The raw
        // request pointer independently makes Scope !Send and !Sync.
        _prior: Option<&'a NativeWorkProducts>,
    }
    impl<'a> Scope<'a> {
        pub(crate) fn new(
            module: Handle,
            machine: Handle,
            prior: Option<&'a NativeWorkProducts>,
            config: &str,
            stats: &'a mut NativeProductStats,
            failed: &'a mut bool,
        ) -> Self {
            let prior_ptr = prior.map_or(ptr::null(), |prior| prior.inner.0.cast_const());
            // SAFETY: module/machine stay live until this local guard drops. CPP
            // copies the length-delimited config and only reads immutable prior.
            let raw = unsafe {
                ZilNativeProductsCreateRequest(
                    module,
                    machine,
                    prior_ptr,
                    config.as_ptr().cast(),
                    config.len(),
                )
            };
            Self {
                raw,
                stats,
                failed,
                _prior: prior,
            }
        }
        pub(crate) fn phase(&self, phase: u32) {
            if !self.raw.is_null() {
                // SAFETY: current live thread-local scope; no LLVM callback is active.
                unsafe { ZilNativeProductsSetPhase(self.raw, phase) }
            }
        }
        pub(crate) fn replay_failed(&mut self) -> bool {
            if !self.raw.is_null() {
                // SAFETY: read only after synchronous LLVM invocation has returned.
                *self.failed |= unsafe { ZilNativeProductsFailed(self.raw) } != 0;
            }
            *self.failed
        }
        pub(crate) fn finish(mut self) -> Option<NativeWorkProducts> {
            if self.raw.is_null() {
                return None;
            }
            let raw = std::mem::replace(&mut self.raw, ptr::null_mut());
            let mut stats = WireStats::default();
            // SAFETY: all native phases succeeded; Finish consumes this request and
            // restores prior TLS. No native owner escapes in the returned product.
            let product = unsafe { ZilNativeProductsFinish(raw, &mut stats) };
            *self.stats = stats.into();
            (!product.is_null()).then(|| NativeWorkProducts {
                inner: Arc::new(Product(product)),
            })
        }
    }
    impl Drop for Scope<'_> {
        fn drop(&mut self) {
            if !self.raw.is_null() {
                self.replay_failed();
                let mut stats = WireStats::default();
                // SAFETY: Abort consumes exactly this live request, before LLVM
                // owners die, and restores TLS on every error/cancellation/unwind.
                unsafe { ZilNativeProductsAbort(self.raw, &mut stats) }
                self.raw = ptr::null_mut();
                *self.stats = stats.into();
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn native_stats_wire_matches_the_eighteen_u64_c_fields() {
            assert_eq!(std::mem::size_of::<WireStats>(), 18 * 8);
            assert_eq!(
                std::mem::align_of::<WireStats>(),
                std::mem::align_of::<u64>()
            );
            assert_eq!(std::mem::offset_of!(WireStats, failed_replays), 17 * 8);
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<NativeWorkProducts>();
        }
    }
}

#[cfg(feature = "native-products")]
pub(crate) use enabled::{Product, Scope};

// Native-only builds do not reference the custom C++ adapter or its DSO hooks.
// Ordinary compile_input always passes None; this inert type keeps that shared
// implementation free of a second, divergent LLVM compilation pipeline.
#[cfg(not(feature = "native-products"))]
pub(crate) struct Scope<'a>(std::marker::PhantomData<&'a mut crate::NativeProductStats>);
#[cfg(not(feature = "native-products"))]
impl<'a> Scope<'a> {
    pub(crate) fn new(
        _module: super::Handle,
        _machine: super::Handle,
        _prior: Option<&'a crate::NativeWorkProducts>,
        _config: &str,
        _stats: &'a mut crate::NativeProductStats,
        _failed: &'a mut bool,
    ) -> Self {
        Self(std::marker::PhantomData)
    }
    pub(crate) fn phase(&self, _phase: u32) {}
    pub(crate) fn replay_failed(&mut self) -> bool {
        false
    }
    pub(crate) fn finish(self) -> Option<crate::NativeWorkProducts> {
        None
    }
}
