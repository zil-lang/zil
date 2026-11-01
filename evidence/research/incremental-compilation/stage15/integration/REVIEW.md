# Integration closeout review

**Pass.** Reviewer `/root/stage8_rust_queries` independently checked the integration records without running new tests, builds or benchmarks.

- Actual local and remote `main`: `24442feb9e915181dedb4866951da01d0ddf67a4`.
- Accepted CI checkout: `2ba0fad9d7333ab271156816e155606209054e04`.
- Identical complete tree: `10f01a026b9479e1b52277d4591045ace04109e3`.
- Both commits have parents `1cb1b62a381d9f261b0b0e5b5e334d9253a7c57c` and delivery `78cc97f6e2a06b35fb886f8bcdeec3c9ddc0607f`; the delivery remains an ancestor of main.

I recomputed the Git commit object ID from [accepted-merge.commit](accepted-merge.commit), rather than relying only on its JSON description. Its SHA256 is `cc0a508a4447e7d406d4d7e177cd39820bc374b8ea5dfc70de016f313e3116a0`. The stored tree and parents match the actual main commit. `git ls-remote` confirms main at the stated merge and no remote `research/incremental-compilation` ref; the local branch is also absent and the current branch is main.

Fresh GitHub API reads confirm [official run 41327533826](https://github.com/zil-lang/zil/actions/runs/41327533826) and [research run 41327533923](https://github.com/zil-lang/zil/actions/runs/41327533923) completed successfully for delivery `78cc97f6…`. Their recovered records bind actual checkout `2ba0fad9…`, retaining the distinction between PR head and checkout.

The coordinator's independent full-blob verification is recorded in the two `ROOT-VERIFICATION.json` receipts. I checked their consistency with recovery/job records and independently hashed the complete recovered compressed archives:

| Configuration | Archive bytes | Files | SHA256 |
|---|---:|---:|---|
| Official | 77,393,193 | 134,761 | `08fe1039cfcdd325f7e277236d8f42e744c0d1642686ae43deea322e0713ec26` |
| Research | 77,643,872 | 134,932 | `c2afd0ed55898d982d5451cbd38e2506f1d9df07db11a908f8aa3bd1db503340` |

Both source inventories contain the same **3,961** tracked non-evidence inputs. Their paths, modes and Git blob IDs match the accepted/main tree. I also independently read the common source-input archive and verified every file's bytes, SHA256, executable mode and Git blob ID. That archive is 6,411,293 bytes, SHA256 `3f0678443e575d0ef6e912c935fe2362de3f25f3cf60752a1b43bffb1fdd1358`. The compact job logs each reproduce 6,535 Rust harness successes with zero failures or ignored tests, consistent with workspace 3,283 + reference 3,188 + text-reference 64. The compact logs intentionally omit transport CHUNK payloads and are not substitutes for the recovered full archives.

Receipt SHA256 values checked here are `31efe0c7f246657ae2820258e1c9133770688ad32957a66dab5fe09d061b1fed` (official root verification), `4f7eaf5e9018cf99b0d30183b01b2ce960e73dbd07adecf1ca8665091970df8b` (research root verification), and `123d42b533d6fb6c1f6dca37b361e15a98b5122cdf186be63de1ba1d28812c33` ([acceptance](ACCEPTANCE.json)). Artifact-upload quota failures remain explicit; successful recovery is not relabeled as successful artifact upload. Full archive retention and Actions-log retention limits are accurately stated.

Reusing these two accepted configurations for the identical full merge tree follows [WORKFLOW.md](../../../../../docs/WORKFLOW.md)'s documented evidence-reuse rule. No event-specific compiler, preflight or test-input difference was found. Provenance remains bound to the tested SHA. Additional main runs `41330551490` and `41330551380` are recorded as running, not passed; their repetition is not the basis of this acceptance.

The pending closeout diff is confined to the research task, Stage15 report and evidence/acceptance records. No compiler, test, tool, workflow, dependency or normative-rule input is changed by that closeout. Its metadata-only commit may retain this acceptance without relabeling it as a newly executed test run. The task closes this bounded integration and branch cleanup while expressly leaving general resource/generic/closure semantic incrementality and the broader Rust product goals unfinished. No remaining closeout blocker was found.
