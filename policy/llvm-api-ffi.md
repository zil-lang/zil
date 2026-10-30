# LLVM FFI trust boundary (LLVM 23.1.4 migration)

`zil-verify` and all existing crates retain `forbid(unsafe_code)`. The only new exception is
`zil-llvm-api`, with crate-wide `deny(unsafe_code)` and one `allow(unsafe_code)` on private
`native` module. Cargo policy registers the leaf dependency; no front-end dependencies.
This is a deliberate reviewed-by-author boundary, NOT a claim of independent security audit.

## Ownership audit
Context/Module/Machine/Layout/PassOptions/Buffer have unique RAII owners. Locals drop in
reverse order, module before its context. No raw handle is public or implements Send/Sync.
The target registry is initialized with Once, contexts never shared between CGUs.
LLVMParseIRInContext consumes the buffer on BOTH success and failure (verified in
llvmorg-23.1.4 llvm/lib/IRReader/IRReader.cpp). Messages use LLVMDisposeMessage;
LLVMGetErrorMessage consumes the LLVMErrorRef and uses LLVMDisposeErrorMessage.
The emitted buffer is copied into owned Rust bytes before disposal. No LLVM caller owns
Rust memory. No host pointer width or host CPU features are used for target layout.

## Failure and cancellation
Parse, verifier (pre/post), target mismatch, pass or object failures return errors.
No object is written before complete code generation. Driver staging publishes the IR and
executable transactionally only after linking and a final cancellation check. Existing
successful artifacts are preserved and are NOT returned as the result of a failed request.
Cancellation is cooperative between native phases, not instantaneous interruption inside
a long LLVM pass or running linker. A compiler/LLVM crash is not sandboxed; private temporary
files may remain after process termination, but are not published as successful artifacts.

## Configuration and trusted input
Feature links libLLVM-23; runtime checks the exact tools/llvm/version.txt pin (23.1.4). Linked LLVM and linker are trusted
build inputs. Input is compiler-generated LLVM IR, NOT a hostile-input sandbox interface.
The opt-in local object cache binds compiler and actual LLVM DSO identity; it still requires
current ZIL Verify and machine preflight. A content hash is not verification authority. Pipeline is
explicit default<O0>/default<O2> with pre/post verification, not verify-each-pass. LLVM API
and Clang pipeline equivalence is not asserted; both consume identical checked program IR.

## Version migration
The LLVM23 audit compares all 104 existing declarations (99 direct and five X86 initializers),
enums and ownership against llvmorg-23.1.4. LLVMParseIRInContext2 is not adopted: the existing
LLVMParseIRInContext call retains its consuming-buffer contract. Independent review and actual
runtime evidence are recorded in docs/toolchain/llvm23 and evidence/toolchain/llvm23.
Changing the upstream field in policy/dependencies.json changes normative_hash; historical
receipts retain their original hashes. No new unsafe module or language safety exception is added.
