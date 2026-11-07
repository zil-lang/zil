# Frozen language-gap replay

This directory replays a fixed set of Source bytes against the historical baseline compiler `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`. It includes the preserved recovery inputs and four nearby controls: Copy tuple partial binding, type/life combinations and usize/u64. A legal target that returned Unsupported or SafeReject in that compiler is not a permanent language prohibition. The [frozen baseline](../../../validation/inputs/language-readiness/language-gaps-baseline.json) records 52 Source checks and nine inputs at LLVM API O0/O2, 18 native executions. The later candidate projection is a separate [bound result](https://github.com/zil-lang/zil/blob/dd766f513bd8022e4d2c703f51a9343824150da8/docs/validation/language-gaps-334-final.json).

To reproduce the old observation, build that exact compiler and supply its actual binary:

```sh
python3 tests/language-readiness/language-gap-replay/replay-frozen-baseline.py \
  --zil /absolute/path/to/frozen-zil \
  --output /tmp/zil-frozen-language-gaps.json
```

The collector checks the compiler identity, input hashes and source changes against its fixed baseline. It does not turn a different compiler into the reviewed one merely by passing a SHA argument. It records source checks and native runs separately; the native harness must print `0\n` and exit 0. Machine products stay in temporary directories. A new candidate uses `compare-candidate.py --zil <new CLI> --compiler-source <actual frozen SHA> --output <new file>`; that command checks source classification only and does not repeat the 18 native executions. New observations need their own report and do not overwrite the frozen JSON. Windows, Rust cost, weak memory and the full language gate are outside this replay.
