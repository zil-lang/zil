# Source 532 single-block diagnostic profile

Candidate `532e01141b644d13c6e350a7dd310c50226d19d3` (tree `a380a742003ff011cf4670b2c9997baff4f45f3f`) compared only with main `99035931d358add0eaed6d4535035c38413701dd`.

This is one diagnostic block with one measured pair per case. It is not a formal Rust comparison, matched matrix, statistical result, or acceptance claim. Plan fragments, input-revision work and mixed slices from later source 693 are absent.

All 34 rows validate: 18 measured + 16 preparations, 17 pairs (15 successful, 2 paired SafeReject), 180 native executions. Every successful pair has equal recorded executable and LLVM SHA-256 identities, and every native result exactly matches the expected bytes/exit code. Compiler receipt identities are checked; physical binary rechecks for this replay: {"baseline": true, "candidate": true}.

| Case | Main ms | Candidate ms | Candidate − main ms | Candidate/main |
|---|---:|---:|---:|---:|
| cold | 7456.574 | 7155.033 | -301.541 | 0.960 |
| unchanged | 466.029 | 92.093 | -373.936 | 0.198 |
| private | 1532.967 | 747.465 | -785.501 | 0.488 |
| multi | 2186.818 | 1461.955 | -724.863 | 0.669 |
| broad | 5823.230 | 6663.999 | +840.769 | 1.144 |
| positions | 1329.007 | 510.129 | -818.878 | 0.384 |
| error | 345.317 | 101.726 | -243.591 | 0.295 |
| recovery | 1384.155 | 630.719 | -753.436 | 0.456 |
| restore | 967.936 | 237.722 | -730.215 | 0.246 |

Actual work and remaining costs:

- Private: Types 5 executed/420 reused, Analyze candidate 5/420 with all 425 equal outputs, Verify function proofs 1 checked/422 reused, empty-borrow proofs 1/424 and 112197 values avoided.
- Private publication still writes 4 semantic segments / 1462334 payload bytes, including the whole CandidatePlan tail. FunctionPlan-fragment publication is later source 693 and is absent here.
- All measured backend source-projection traversal/visit/reuse counters are zero: unconsumed projections are omitted. This is not a new dynamic dependency graph; producer graph counters also remain zero.
- Unchanged: exact full Verify receipt reused once. Historical restore: frontend raw hit, no producer/Types/Analyze counters reported, but Verify still checks 1 function and reuses 422; this is neither zero Verify work nor a full Verify receipt hit.
- Private still hashes 21 function inputs, deep-clones 21 normalized items, parses one module and relocates 20 CFGs. Input-revision and mixed-slice work from later source 693 must not be attributed here.
- Cold retains 425 function/22 module production and higher root-only legacy RSS field than main; its wall time happens to improve in this one sample. Broad is slower than main; multi still spends 147040985ns Analyze and 202154332ns Verify despite narrow final changes.
- Source532 unchanged/history paths use raw-owner checkpoints. Resident caches/retained memory persist through all 17 requests (no session clears); this single-block sequence does not establish a global memory or disk quota.

The broad regression is retained (+840.769 ms, +14.44%). The legacy sampled RSS field grows from 457340 to 557672 KiB for cold; cold wall time is 301.541 ms lower in this sample, so no cold wall regression is invented. Multi remains 1461.955 ms despite three final changed functions; its Analyze/Verify costs and all exact counters are retained in JSON.

Unchanged dependency reads validate 25 snapshots/24 dependencies with 2 shared validations. Source restore has `resident_frontend_hits=1`, `producer=null`, `function_types=null`, `initialization=null`; it still has one checked Verify function/empty-borrow proof, 422/424 reused respectively, and lowers one function. Restore is a historical raw-owner frontend hit, not a disk semantic restore (`semantic_load_attempts=0`) or a complete Verify hit.

Controller discrepancy: the helper persisted `.exit=0` and a complete 17-pair valid log, while root observed `write_stdin` reporting exit 1. Both observations remain recorded; the discrepancy is unresolved and the semantic validation does not overwrite the controller exit.

Memory scope: all 34 archived sample files (4457 samples) report exactly one process. The archived sampler silently falls back to its root when `/proc/<pid>/task/<pid>/children` is missing; the current probe reports exists=False, readable=False. Thus `tree_peak_sampled_rss_sum_kib` is a legacy root-only observed field here, not process-tree or total build memory. No total-memory advantage is claimed.

Evidence SHA-256 receipts:

- `out/runs/full-pipeline-profile-o2-r5/rows.jsonl`: `56ce860f4bd203628e1b18cb2a78b1da2eabfca263aaf72b95163af9dbca8f4f` (1384067 bytes).
- `out/runs/full-pipeline-profile-o2-r5/manifest.json`: `4c22c1ac65bf499b3aa34ef2e12ffbdbcca9448e3a6b3ebe06e0c9af7709e2da` (3273 bytes).
- `out/runs/full-pipeline-profile-o2-r5/summary.json`: `96d342c603fe5cb88000eda78028f89143bb32272c8184426e1f2a5a5abb4754` (6127 bytes).
- `out/runs/full-pipeline-profile-o2-r5.log`: `a09f5d2a6cbe0096733f3e7d1d1ecb50563370438d282b95c021bd3fefa65a90` (3400 bytes).
- `out/runs/full-pipeline-profile-o2-r5.exit`: `9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa` (2 bytes).
- `out/runs/full-pipeline-proof-build/candidate-build.json`: `452e186ad3d428a6981007cb440d0f0d7235a65af6420fe981796d9006051e8a` (597 bytes).
- `out/runs/full-pipeline-proof-release-01.log`: `ec0d3a41693a0fc8265d98cbf053e5fc9a0600061f3654926f0a780e48613d13` (574 bytes).
- `out/runs/full-pipeline-profile-o2-r5/native_measure.py`: `7294a9ade9c8f4a198bbeab9d4dd7c4791684e3b2fb45c21b995abb0ea54ba5d` (25234 bytes).
- `out/runs/full-pipeline-profile-o2-r5/native_workloads.py`: `df6e95b48825063b718fd65a0c4ab8afaa940f6880a40215d60ac9105ced6964` (10836 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/0-cold.memory-samples.json`: `0fa26d31f511190c5d979287faecf9f32d389bcb2f4ccfa8d26baa6146397ec4` (65802 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/0-cold.memory-samples.json`: `55e74fbb26187d1707bd6bbb90e2f7b9f8f05158fdbffb10e420e8adc4999ce7` (63358 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/1-prepare-cold.memory-samples.json`: `94c3459612a6bea9b131ccb4bcc5827178e2a9f3fec4a0401d68b62bde09b1a1` (3086 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/1-prepare-cold.memory-samples.json`: `30406d0d59edf8ad68b283c82d61700424a3b6928969fb1ee3bcda1be46faa39` (7900 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/1-unchanged.memory-samples.json`: `5c75956c385181b7f1e403c321396c88a60cb04d2cff50856fb435d53e073973` (810 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/1-unchanged.memory-samples.json`: `83ea079cc9c584327d465be122a00f8bc36300b2b52a73e953ac0f3930537b64` (4171 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/2-prepare-cold.memory-samples.json`: `704f2bc352592dc73cbd1a3de48e9b8b0d00a18a26c96fe466babb4c0f3f44f1` (8714 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/2-prepare-cold.memory-samples.json`: `fdbc7d38f1d845c43ca7c315cb562198ad0c5e13b33fcc6fb585e65f11918dc7` (991 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/2-private.memory-samples.json`: `9cab37150d8be73312f3cfe64fe0e6604729c99ee0111a6b0a081e90166554fa` (13589 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/2-private.memory-samples.json`: `cfc7254733447b98068e201f9695ef9ff55bbcde6b5a0cd2037984c5eafea135` (6622 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/3-prepare-cold.memory-samples.json`: `a55e7395a68708e076d7de5f8d24d3daed0e2c0d97cd6524d752af187d21e8fc` (1717 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/3-prepare-cold.memory-samples.json`: `0c8f395b9b5d1073f6e5740fd72822ab04dcbb468be7399f3e039d9742141e01` (4448 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/3-multi.memory-samples.json`: `3d578aea2bb6081ed03a31fa917b31b7b7929ef6b849385b860203d6d66232c9` (12670 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/3-multi.memory-samples.json`: `92b2f0af68f44a87ef559de758bd52adca762c5be650fdfccd93c316fd16da54` (19007 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/4-prepare-cold.memory-samples.json`: `74483e6b119b52d52a2851dd08e6cc58834db14f1a69cf900f90f3691a1247d2` (4352 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/4-prepare-cold.memory-samples.json`: `49b3ad7e64338bae7fa7bae2d1680fa541ca9f6b381ba2bb3f7145d6656bcb5c` (5900 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/4-broad.memory-samples.json`: `725b52a93fe1612379d4a42e553b833f99bccb0f7dde4af762bda6dfd64145c0` (51631 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/4-broad.memory-samples.json`: `9416437ee564cc2746c5d66dc20946dd0b82d7b8aef0884af9a5bbde221acaa8` (59737 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/5-prepare-cold.memory-samples.json`: `c7fb9b0fcce4d6560c6f7d04be4b4b4c5dbd75ab2821b72b20002961f5cee49a` (7804 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/5-prepare-cold.memory-samples.json`: `552b02967d96efebd6ec2a32a497bc104bcb30b75532a39477cc63962e05e2fe` (4804 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/5-positions.memory-samples.json`: `152c5dd7be1a1955243153240d1854b646477266f509fdb7bf3dde882781ad1f` (4588 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/5-positions.memory-samples.json`: `c55e0faac230101b537c442e023bbbdf809ab95fc78e66843e24ee1c9065bd04` (11880 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/6-prepare-cold.memory-samples.json`: `cff113f277218babd3822b3a43af60f7562be81de1f5dfd8674fb32ab3fad37b` (4403 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/6-prepare-cold.memory-samples.json`: `df757d2bddc372b1a2dc7ee8b633e1e08d755c3275df086ca5a1c87128faf366` (1002 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/6-error.memory-samples.json`: `bbe4bafb2a25120bbd1f52c0edc2977bf080ef53e45485148de6a09ec1a2ca60` (3119 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/6-error.memory-samples.json`: `438f7416021584c1c777bb2393712969f97120816251202193554692f301e658` (910 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/7-prepare-error.memory-samples.json`: `f4a1c4c12bbf93c63474d191e2a14d390355cfb57cbd9834b3bddc5ab5c60c9a` (275 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/7-prepare-error.memory-samples.json`: `3aacd12f39f2bf4e921137d3c6efa28e3e277c338cda415ddddd84b30d2f74c4` (274 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/7-recovery.memory-samples.json`: `5082334d454f0244ec1d057ec391bd4da532ca348c05a387be57511dedac19af` (5687 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/7-recovery.memory-samples.json`: `1f0d93152543c5e35a083f0539bd61074dcc7e53800ea99bf90363759b5e2371` (12436 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/8-prepare-private.memory-samples.json`: `32741e04108400a83b92a6ffaf3570f7e05f5b7c4756e5991a902fb368b0d086` (4221 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/8-prepare-private.memory-samples.json`: `e835c376ee017adb23e7d6e9de3fe075bab1842a3554b2ffcf4648319dc36dc1` (1648 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-baseline/8-restore.memory-samples.json`: `2bcaa47e60d3ffac588060e1a2dc658f32aac01515faf3698483cf08e9e7ec97` (8631 bytes).
- `out/runs/full-pipeline-profile-o2-r5/block-0-candidate/8-restore.memory-samples.json`: `4294e54ba5bf249f859e58ed9510813e03ef59d866096007dc46dea32ba8b4f6` (2197 bytes).

Candidate compiler SHA-256: `df7b2bb1a6807efa47e2db0df87150a6395d9c92c0f95b1414f48d5a026c4e6a`; main compiler: `e5fb6fdfdd9bd1a2a7d064519defaf860e0207d939924ab0fab6fe59044454c7`.

Validation script SHA-256: `161f8402c00b19ca36d5d649d47e5c8c229ac28051089fab511010c6ced75bb5`. Reproduce the data assertions and summaries with `python tools/research/incremental-demand/profile_532_validate.py --repo .` (the repository root is required). The companion JSON contains all 34 reduced rows, all reported frontend/Verify/backend/publication counters, nine measured comparisons and all pair identity receipts.

No build, measurement, raw mutation or source edit was performed by this summarizer. These r5 raw files are local; remote archive durability needs its own later receipt.
