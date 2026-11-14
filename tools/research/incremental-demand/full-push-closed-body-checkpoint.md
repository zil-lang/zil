# ClosedBody unified acceptance at 021997

Compiled `021997a7d0b68e61b86fd084dba319583b6dc464` / binary `8ec292e47c55a24e5370f623355d6360aae2c938d43c78c77374b531a675a468`. Test-only followup `e5e14cc0fe4ee6377436b4364bb05b89f66c553b` is separately bound; historical98fa results remain unchanged.

Unified gates actually passed native86/backend269/driver211/Source9/retained1/strictClippy/fmt/release. First offline-dependency failure, zero-execution ignored test, missing lexical SDK-directory refusal and successful retry are preserved. No LLVM rebuild.

Non-author review found no confirmed P0/P1 authorization-chain blocker. Its P2 attribution gap was fixed by a module with one closed leaf and no WorldBound definitions: no-arena prior has nonzero IR/MIR/MC replay, exact full object, zero failed replay/fallback, current LLVM verification; producer World projections7, consumer0. Followup focused test/Clippy/fmt pass; no unrelated full suite rerun.

Two blocks per mode, only measured target_request rows; parents excluded. Actual main9170/candidate/Rust only, original425, CGU16/O2/jobs1. Medians in ms; paired GM below1 means candidate shorter in this window.

|mode|request|main|candidate|Rust|GM c/main|GM c/Rust|
|---|---|---:|---:|---:|---:|---:|
|check|cold|1211.85|990.79|1169.73|0.862|0.858|
|check|unchanged|98.54|2.98|13.51|0.030|0.226|
|check|private|783.07|69.54|287.56|0.089|0.242|
|check|multi|624.22|86.82|315.21|0.140|0.279|
|check|broad|874.58|606.02|975.50|0.689|0.614|
|check|positions|1063.61|455.70|835.82|0.429|0.552|
|check|error|506.33|47.99|296.08|0.094|0.162|
|check|recovery|589.43|75.32|198.33|0.125|0.369|
|check|restore|110.44|26.23|254.72|0.240|0.105|
|o2|cold|5984.14|4651.10|5818.32|0.773|0.795|
|o2|unchanged|637.77|13.00|12.63|0.020|0.984|
|o2|private|1664.31|358.26|479.67|0.218|0.750|
|o2|multi|1802.02|559.63|975.53|0.312|0.573|
|o2|broad|4748.36|4711.57|5438.66|0.993|0.867|
|o2|positions|1803.91|899.28|1205.24|0.491|0.735|
|o2|error|392.64|68.79|190.19|0.173|0.356|
|o2|recovery|1553.19|366.21|499.19|0.234|0.730|
|o2|restore|568.41|71.21|504.94|0.125|0.141|

Both windows102valid rows=54measured+48parents, controller0. Native1080 oracles and30 wholeELF/exports pairs pass. All178 native artifact blobs rehashed. Full raw LLVM is retained; reviewed IR representation changes do not weaken wholeELF/current Verify/optimization acceptance.

Negative observations and limits: Global ReactiveGraph still evaluates BodyKey; complete central point-to-point propagation is unfinished. Only two blocks per cell; no comprehensive/stable Rust lead. O2 unchanged block1 candidate17.088ms vsRust14.196ms; candidate median13.00ms vsRust12.63ms. Check cold block1 candidate909.346ms vsmain815.902ms; O2 broad block0 candidate4567.622ms vsmain4510.158ms. No main merge, default research flags unchanged; current independent Verify, budgets, fullO2 and quality gates unchanged.

ZIL resident vs Rust new Cargo/rustc process and LLVM23.1.4 vs23.1.1 remain different boundaries. Source writes/hash/artifact capture/native oracles are outside compile timer; first profile admission inside cold; OS caches not flushed. No cross-window causal gain or memory advantage inferred.

Raw custody: local chunked exact-byte archive with manifest and fresh restore verification; remote publication blocked by GitHub write authentication. SDK18/native-link fixed recovery references remain authoritative; do not substitute another DSO.
