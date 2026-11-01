# Independent closed-executable review

Verdict: **Pass**, limited to correctness/ABI/cache review of `c6a60d3ec15db9df6953d4e38f640ba23b1d8a94` relative to `5189f09750c138eb61a4c86bea3654d7f0e57f6f`. Reviewer: `/root/review_incremental_slice1`. No performance or product gate verdict.

Default remains false. The CLI restricts opt-in to complete Linux x86_64 builds and the backend independently checks target. Current VerifiedModule/LIR binding precedes machine transformation. Only exact current nonforeign `zil.fn.<id>` External definitions are eligible. Cross-CGU calls, function addresses and recursively nested global initializer references preserve external linkage. Existing local references, global data, foreign declarations, main/wrappers and unlisted helper names are untouched. Duplicate candidate definitions retain the ordinary rejection path. No function address merging or unnamed-address permission is introduced.

The single-unit Clang path consumes transformed unit 0; public diagnostic emission remains unchanged. Object keys include actual linkage in the full ordered machine structure and existing compiler/target/optimization identity. Policy alone grants no cache/verification capability. Existing current preflight, cancellation and staged artifact publication remain in effect.

18 independent tests passed from an exact Git-exported snapshot, in an independent target: backend ABI/boundary/cache 6; machine partition/internalization 9; real-source policy/cache 1; real owned-closure/resource/address 1; CLI native/nm/argument controls 1. Source and log hashes and exact commands are in `closed-review.json`. Production files were not modified.

This is an explicit narrower export contract, not the default compiler behavior and not whole-program LTO. Cross-unit definitions can remain exported. Model cases are not presented as source acceptance; real source tests are distinguished. Timing windows have not begun and no speed claim is made.
