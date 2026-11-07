# Initial independent Source baseline

Fixture: `project/main.zil` imports four real modules (`api`, `cursor`, `consumer`, `other`). The original Source files are preserved in `project/`.

Command, run on 2026-11-06 before G2.4.1 implementation:

```sh
source /workspace/.zil-env/env.sh
/workspace/.zil-env/target-release-G9.3.2/release/zil check tests/language-readiness/G2.4.1/project/main.zil --json
```

Exit 3; `status=Unsupported`, `phase=expand`, diagnostic `E-UNSUPPORTED`: `life is supported only by ordinary direct functions and concrete inherent methods`. The diagnostic points to `api.zil` (file 1, byte span 52..110), the ordinary trait declaration. The CLI loaded all five real modules and completed parsing before the expected old boundary. This is a baseline failure, not acceptance evidence.

Input fingerprint: `e8f7c9b4dda05b1737d84bc8e9369637f9ff751a93847f44a11efed199de9ce4`.
