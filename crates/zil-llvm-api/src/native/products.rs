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
        mir_key_ns: u64,
        mir_parse_ns: u64,
        mir_restore_validate_ns: u64,
        mir_capture_ns: u64,
        retained_bytes: u64,
        failed_replays: u64,
        world_projection_direct: u64,
        world_projection_fallback: u64,
        tli_projection_builds: u64,
        tli_projection_reuses: u64,
        ir_key_ns: u64,
        ir_unpack_ns: u64,
        ir_replay_ns: u64,
        ir_parse_ns: u64,
        ir_capture_ns: u64,
        ir_pack_ns: u64,
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
                mir_key_ns: value.mir_key_ns,
                mir_parse_ns: value.mir_parse_ns,
                mir_restore_validate_ns: value.mir_restore_validate_ns,
                mir_capture_ns: value.mir_capture_ns,
                retained_bytes: value.retained_bytes,
                failed_replays: value.failed_replays,
                world_projection_direct: value.world_projection_direct,
                world_projection_fallback: value.world_projection_fallback,
                tli_projection_builds: value.tli_projection_builds,
                tli_projection_reuses: value.tli_projection_reuses,
                ir_key_ns: value.ir_key_ns,
                ir_unpack_ns: value.ir_unpack_ns,
                ir_replay_ns: value.ir_replay_ns,
                ir_parse_ns: value.ir_parse_ns,
                ir_capture_ns: value.ir_capture_ns,
                ir_pack_ns: value.ir_pack_ns,
                ..Self::default()
            }
        }
    }
    unsafe extern "C" {
        #[cfg(test)]
        fn ZilNativeProductsOwnedUseOrderCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsIncomingCallerCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsAttributeMetadataCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsModuleHeaderCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeAnalysisFactsTLIMemoCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeAnalysisUnsupportedKeysCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeIRScopeTimerCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsUnchangedStageCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsEligibilityProducerCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsTypeAndOwnerCounterexamples() -> c_int;
        #[cfg(test)]
        fn ZilNativePackedStageFrameCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativePackedStageCodecCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativePackedStageMetadataCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativePackedStageEntryFilterCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeStageEconomicsCounterexample() -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsStageAlwaysForTests(enabled: c_int) -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsAnalysisDiagnostics(enabled: c_int) -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsMachineDiagnostics(enabled: c_int) -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsMachineNumberingTests(enabled: c_int) -> c_int;
        #[cfg(test)]
        fn ZilNativeProductsMachineNumberingTestResults() -> u32;
        #[cfg(test)]
        fn ZilNativeProductsMachineBundleTestResults() -> u32;
        #[cfg(test)]
        fn ZilNativeProductsMachineTypedBodyTestResults() -> u32;
        #[cfg(all(test, feature = "text-reference"))]
        fn ZilNativeProductsMachineAlignmentTest(max_bytes: c_int) -> c_int;
        #[cfg(all(test, feature = "text-reference"))]
        fn ZilNativeProductsMachineFrameTestResults() -> u32;
        #[cfg(all(test, feature = "text-reference"))]
        fn ZilNativeProductsMachineFrameObservationResults() -> u32;
        fn ZilNativeProductsSuspend() -> Handle;
        fn ZilNativeProductsResume(previous: Handle);
        fn ZilNativeProductsCreateRequest(
            module: Handle,
            machine: Handle,
            prior: *const std::ffi::c_void,
            config: *const c_char,
            config_len: usize,
        ) -> Handle;
        fn ZilNativeProductsSetPhase(request: Handle, phase: c_uint);
        fn ZilNativeProductsFailed(request: *const std::ffi::c_void) -> c_int;
        fn ZilNativeProductsFinish(
            request: Handle,
            stats: *mut WireStats,
            failed: *mut c_int,
        ) -> Handle;
        fn ZilNativeProductsAbort(request: Handle, stats: *mut WireStats);
        fn ZilNativeProductsDispose(products: Handle);
    }

    /// A whole-compile guard. Declared before all LLVM owners so those owners
    /// are destroyed before a suspended outer request can become current again.
    /// Raw TLS tokens keep this guard !Send/Sync, including a null prior token.
    pub(crate) struct Suspension(Handle);
    impl Suspension {
        pub(crate) fn new() -> Self {
            // SAFETY: same-thread lexical save/suspend of optional native TLS.
            // Missing extension capability is a no-op and still returns a valid
            // token (possibly null) for the matching Resume operation.
            Self(unsafe { ZilNativeProductsSuspend() })
        }
    }
    impl Drop for Suspension {
        fn drop(&mut self) {
            // SAFETY: inner Scope and all LLVM owners have already died. Restore
            // exactly the outer TLS token saved by this non-cloneable guard.
            unsafe { ZilNativeProductsResume(self.0) }
        }
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
    // Frame products own a complete MachineFrameInfo value assignment. Every
    // StackObject Alloca is null; retained Save/Restore maps are cleared and
    // production admits only final post-PEI empty maps. Owner-map scratch values
    // retain only block ordinals and scalar CalleeSavedInfo. MMO values remove
    // their PSV pointer and reject all IR/metadata owners; replay allocates them
    // using current Stack/FixedStack PSV and the complete original FI table.
    // CFI variants are deeply copied only with all labels/source locations null.
    // No historical BB, MI or MCContext pointer survives these value products.
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
            // Fail closed if the adapter cannot confirm final machine-product
            // binding. Finish can discover a late replay mismatch after Emit.
            let mut failed = 1;
            // SAFETY: Finish consumes this request and restores prior TLS. Its
            // separate status covers failures found while closing machine state;
            // a null product alone is never treated as proof of successful replay.
            let product = unsafe { ZilNativeProductsFinish(raw, &mut stats, &mut failed) };
            #[cfg(test)]
            match NEXT_FINISH_FAILURE.with(|value| value.replace(0)) {
                1 => stats.failed_replays = stats.failed_replays.saturating_add(1),
                2 => failed = 1,
                _ => {}
            }
            *self.failed |= failed != 0 || stats.failed_replays > 0;
            *self.stats = stats.into();
            (!product.is_null()).then(|| NativeWorkProducts {
                inner: Arc::new(Product(product)),
                retained_payload_bytes: self.stats.retained_bytes,
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

    /// Explicit fixture override of optional economic refusal, not replay checks.
    #[cfg(test)]
    pub(crate) struct StageAlwaysGuard(c_int, std::marker::PhantomData<*mut ()>);
    #[cfg(test)]
    impl Drop for StageAlwaysGuard {
        fn drop(&mut self) {
            // SAFETY: restore this same thread's prior fixture setting.
            unsafe { ZilNativeProductsStageAlwaysForTests(self.0) };
        }
    }
    #[cfg(test)]
    pub(crate) fn stage_always_for_test() -> StageAlwaysGuard {
        // SAFETY: newly constructed requests snapshot this explicit TLS hint.
        // It only disables refusal; complete old admission/replay still runs.
        let prior = unsafe { ZilNativeProductsStageAlwaysForTests(1) };
        StageAlwaysGuard(prior, std::marker::PhantomData)
    }

    #[cfg(test)]
    pub(crate) struct AnalysisDiagnosticsGuard(c_int, c_int, std::marker::PhantomData<*mut ()>);
    #[cfg(test)]
    impl Drop for AnalysisDiagnosticsGuard {
        fn drop(&mut self) {
            // SAFETY: restore the exact prior mode on this same thread, including
            // panic/cancellation paths. No LLVM owner is read by this operation.
            unsafe {
                ZilNativeProductsMachineDiagnostics(self.1);
                ZilNativeProductsAnalysisDiagnostics(self.0);
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn analysis_diagnostics_for_test() -> AnalysisDiagnosticsGuard {
        // SAFETY: explicit test-only thread-local diagnostic mode; not permission
        // to replay any analysis or change the production optimization pipeline.
        let prior = unsafe { ZilNativeProductsAnalysisDiagnostics(1) };
        // SAFETY: independently save/enable the machine diagnostic TLS mode;
        // neither setter grants replay permission or changes optimization.
        let machine_prior = unsafe { ZilNativeProductsMachineDiagnostics(1) };
        AnalysisDiagnosticsGuard(prior, machine_prior, std::marker::PhantomData)
    }

    /// Explicit same-thread scratch checks. The bitmask reports observations;
    /// it is not permission to reuse a machine function or bypass verification.
    #[cfg(test)]
    pub(crate) struct MachineNumberingTestsGuard(c_int, std::marker::PhantomData<*mut ()>);
    #[cfg(test)]
    impl MachineNumberingTestsGuard {
        pub(crate) fn results(&self) -> u32 {
            // SAFETY: synchronously read the test-only status on the same thread
            // after LLVM returned; no native owner or callback crosses this call.
            unsafe { ZilNativeProductsMachineNumberingTestResults() }
        }
        pub(crate) fn bundle_results(&self) -> u32 {
            // SAFETY: same-thread read of the scratch-only observation mask;
            // no LLVM owner or proof is exposed by this result.
            unsafe { ZilNativeProductsMachineBundleTestResults() }
        }
        pub(crate) fn typed_body_results(&self) -> u32 {
            // SAFETY: same-thread read of explicit scratch checks after LLVM
            // returned. The mask contains no native owner or reuse authority.
            unsafe { ZilNativeProductsMachineTypedBodyTestResults() }
        }
        #[cfg(feature = "text-reference")]
        pub(crate) fn frame_results(&self) -> u32 {
            // SAFETY: same-thread read of the completed scratch-only mask.
            unsafe { ZilNativeProductsMachineFrameTestResults() }
        }
        #[cfg(feature = "text-reference")]
        pub(crate) fn frame_observations(&self) -> u32 {
            // SAFETY: an observation mask contains no foreign owner or authority.
            unsafe { ZilNativeProductsMachineFrameObservationResults() }
        }
    }
    #[cfg(test)]
    impl Drop for MachineNumberingTestsGuard {
        fn drop(&mut self) {
            // SAFETY: exactly restore this thread's preceding enable mode on
            // success, cancellation or panic. The guard cannot move threads.
            unsafe {
                ZilNativeProductsMachineNumberingTests(self.0);
            }
        }
    }
    #[cfg(test)]
    pub(crate) fn machine_numbering_tests_for_test() -> MachineNumberingTestsGuard {
        // SAFETY: explicitly enable only independent scratch-MF validation at
        // the first actual AsmPrinter boundary; production default stays off.
        let prior = unsafe { ZilNativeProductsMachineNumberingTests(1) };
        MachineNumberingTestsGuard(prior, std::marker::PhantomData)
    }

    /// Explicit test policy applied only at cold AsmPrinter entry. Its value is
    /// bound into the native machine request so warm reuse must restore it.
    #[cfg(all(test, feature = "text-reference"))]
    pub(crate) struct MachineAlignmentTestGuard(c_int, std::marker::PhantomData<*mut ()>);
    #[cfg(all(test, feature = "text-reference"))]
    impl Drop for MachineAlignmentTestGuard {
        fn drop(&mut self) {
            // SAFETY: restore the same thread's previous test-only scalar mode.
            unsafe { ZilNativeProductsMachineAlignmentTest(self.0) };
        }
    }
    #[cfg(all(test, feature = "text-reference"))]
    pub(crate) fn machine_alignment_test_for_test(max_bytes: c_int) -> MachineAlignmentTestGuard {
        assert!(max_bytes >= -1);
        // SAFETY: no LLVM owner crosses this scalar TLS setter. Only explicit
        // test scopes enable the policy; -1 restores production's disabled mode.
        let prior = unsafe { ZilNativeProductsMachineAlignmentTest(max_bytes) };
        MachineAlignmentTestGuard(prior, std::marker::PhantomData)
    }

    #[cfg(test)]
    thread_local! {
        static NEXT_FINISH_FAILURE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
    }
    // Thread-local, test-only injection happens *after* actual CPP Finish. It
    // exercises the late-status/counter boundary with real optimized objects.
    #[cfg(test)]
    pub(crate) struct LateFailureGuard(u8, std::marker::PhantomData<*mut ()>);
    #[cfg(test)]
    impl Drop for LateFailureGuard {
        fn drop(&mut self) {
            NEXT_FINISH_FAILURE.with(|value| value.set(self.0));
        }
    }
    #[cfg(test)]
    pub(crate) fn fail_next_finish_for_test(counter_only: bool) -> LateFailureGuard {
        let old = NEXT_FINISH_FAILURE.with(|value| value.replace(if counter_only { 1 } else { 2 }));
        LateFailureGuard(old, std::marker::PhantomData)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn packed_stage_frames_keep_all_bytes_and_reject_malformed_lengths() {
            // SAFETY: the fixture returns only a scalar and retains no foreign owner.
            assert_eq!(unsafe { ZilNativePackedStageFrameCounterexample() }, 1);
        }

        #[test]
        fn installed_stage_codec_is_lossless_and_rejects_corrupted_frames() {
            // SAFETY: all codec buffers are owned by the fixture; -1 is not a pass.
            assert_eq!(unsafe { ZilNativePackedStageCodecCounterexample() }, 1);
        }

        #[test]
        fn packed_stage_keeps_every_typed_analysis_and_replay_field() {
            // SAFETY: the fixture contains its LLVM metadata and returns a scalar.
            assert_eq!(unsafe { ZilNativePackedStageMetadataCounterexample() }, 1);
        }

        #[test]
        fn complete_current_projections_bind_eligibility_reuse() {
            // SAFETY: the counterexample owns every LLVM object and returns a scalar.
            assert_eq!(
                unsafe { ZilNativeProductsEligibilityProducerCounterexample() },
                1
            );
        }

        #[test]
        fn named_type_definitions_and_current_caller_owners_are_bound() {
            // SAFETY: all malformed-owner and distinct-type fixtures remain local.
            assert_eq!(unsafe { ZilNativeProductsTypeAndOwnerCounterexamples() }, 1);
        }

        #[test]
        fn unsupported_analysis_keys_reject_before_touching_any_results() {
            // SAFETY: the fixture owns its empty FAM and returns no LLVM owner.
            assert_eq!(
                unsafe { ZilNativeAnalysisUnsupportedKeysCounterexample() },
                1
            );
        }

        #[test]
        fn unchanged_stage_keeps_body_owners_and_validates_current_analysis_exit() {
            // SAFETY: all IR/FAM owners and corrupt-result negatives stay local.
            assert_eq!(
                unsafe { ZilNativeProductsUnchangedStageCounterexample() },
                1
            );
        }

        #[test]
        fn current_module_headers_preserve_values_and_reject_body_owners() {
            // SAFETY: all fixture LLVM owners remain inside the C++ call.
            assert_eq!(unsafe { ZilNativeProductsModuleHeaderCounterexample() }, 1);
        }

        #[test]
        fn request_local_tli_projection_matches_full_current_facts() {
            // SAFETY: the fixture owns each FAM and tests explicit token expiry.
            assert_eq!(unsafe { ZilNativeAnalysisFactsTLIMemoCounterexample() }, 1);
        }

        #[test]
        fn complete_call_attributes_and_metadata_distinguish_equal_function_text() {
            let _diagnostics = analysis_diagnostics_for_test();
            // SAFETY: the fixture owns all LLVM values and returns only its
            // complete equality/rejection observations as a scalar result.
            let result = unsafe { ZilNativeProductsAttributeMetadataCounterexample() };
            assert_eq!(result, 1);
        }

        #[test]
        fn owned_use_order_distinguishes_equal_official_function_text() {
            // SAFETY: the synchronous C++ counterexample owns and destroys its
            // LLVM context, module and uses before returning a scalar result.
            // A successful observation grants no native reuse authority.
            let result = unsafe { ZilNativeProductsOwnedUseOrderCounterexample() };
            assert_eq!(
                result, 1,
                "real reversed LLVM use order must collide in official text, differ in the complete key, and restore exactly"
            );
        }

        #[test]
        fn unmappable_incoming_caller_never_authorizes_callee_reuse() {
            let _diagnostics = analysis_diagnostics_for_test();
            // SAFETY: the independent C++ fixture retains every LLVM owner
            // locally and returns only its complete counterexample outcome.
            let result = unsafe { ZilNativeProductsIncomingCallerCounterexample() };
            assert_eq!(
                result, 1,
                "plain caller must be admitted, while distinct blockaddress callers must both reject reuse rather than share an empty key"
            );
        }

        #[test]
        fn packed_stage_entry_summary_can_only_reject_before_full_decode_and_equality() {
            // SAFETY: local immutable packets and fake digest collisions only;
            // no LLVM pass or compilation is skipped by the fixture itself.
            assert_eq!(
                unsafe { ZilNativePackedStageEntryFilterCounterexample() },
                1
            );
        }

        #[test]
        fn stage_economics_bounds_probes_and_keeps_original_nested_pipeline() {
            // SAFETY: the fixture owns its real FPM/FAM/TLS scope and checks
            // deterministic synthetic costs; it does not measure a workload.
            assert_eq!(unsafe { ZilNativeStageEconomicsCounterexample() }, 1);
        }

        #[test]
        fn ir_scope_raii_records_returns_stops_and_nested_cleanup() {
            // SAFETY: the fixture uses only a deterministic fake monotonic
            // clock and local counters; it runs no compiler or wall benchmark.
            assert_eq!(unsafe { ZilNativeIRScopeTimerCounterexample() }, 1);
        }

        #[test]
        fn native_stats_wire_preserves_prefix_and_appends_six_ir_scope_fields() {
            assert_eq!(std::mem::size_of::<WireStats>(), 32 * 8);
            assert_eq!(
                std::mem::align_of::<WireStats>(),
                std::mem::align_of::<u64>()
            );
            assert_eq!(std::mem::offset_of!(WireStats, mir_key_ns), 16 * 8);
            assert_eq!(std::mem::offset_of!(WireStats, mir_parse_ns), 17 * 8);
            assert_eq!(
                std::mem::offset_of!(WireStats, mir_restore_validate_ns),
                18 * 8
            );
            assert_eq!(std::mem::offset_of!(WireStats, mir_capture_ns), 19 * 8);
            assert_eq!(std::mem::offset_of!(WireStats, failed_replays), 21 * 8);
            assert_eq!(
                std::mem::offset_of!(WireStats, world_projection_direct),
                22 * 8
            );
            assert_eq!(
                std::mem::offset_of!(WireStats, world_projection_fallback),
                23 * 8
            );
            assert_eq!(
                std::mem::offset_of!(WireStats, tli_projection_builds),
                24 * 8
            );
            assert_eq!(
                std::mem::offset_of!(WireStats, tli_projection_reuses),
                25 * 8
            );

            assert_eq!(std::mem::offset_of!(WireStats, ir_key_ns), 26 * 8);
            assert_eq!(std::mem::offset_of!(WireStats, ir_unpack_ns), 27 * 8);
            assert_eq!(std::mem::offset_of!(WireStats, ir_replay_ns), 28 * 8);
            assert_eq!(std::mem::offset_of!(WireStats, ir_parse_ns), 29 * 8);
            assert_eq!(std::mem::offset_of!(WireStats, ir_capture_ns), 30 * 8);
            assert_eq!(std::mem::offset_of!(WireStats, ir_pack_ns), 31 * 8);
            let observed = NativeProductStats::from(WireStats {
                ir_key_ns: 11,
                ir_unpack_ns: 12,
                ir_replay_ns: 30,
                ir_parse_ns: 4,
                ir_capture_ns: 50,
                ir_pack_ns: 6,
                ..WireStats::default()
            });
            assert_eq!(observed.ir_key_ns, 11);
            assert_eq!(observed.ir_unpack_ns, 12);
            assert_eq!(observed.ir_replay_ns, 30);
            assert_eq!(observed.ir_parse_ns, 4);
            assert_eq!(observed.ir_capture_ns, 50);
            assert_eq!(observed.ir_pack_ns, 6);

            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<NativeWorkProducts>();
        }
    }
}

#[cfg(feature = "native-products")]
pub(crate) use enabled::{Product, Scope, Suspension};
#[cfg(all(test, feature = "native-products"))]
pub(crate) use enabled::{
    analysis_diagnostics_for_test, fail_next_finish_for_test, machine_numbering_tests_for_test,
    stage_always_for_test,
};

#[cfg(all(test, feature = "native-products", feature = "text-reference"))]
pub(crate) use enabled::machine_alignment_test_for_test;

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

#[cfg(not(feature = "native-products"))]
pub(crate) struct Suspension;
#[cfg(not(feature = "native-products"))]
impl Suspension {
    pub(crate) fn new() -> Self {
        Self
    }
}
