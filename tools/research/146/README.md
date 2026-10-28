# 146 复现

1. 从bundle创建两个worktree，A为c1ec559d，C为sandbox/research-146的最终提交。Rust1.99.1、Clang17。
2. 两侧使用各自绝对CARGO_TARGET_DIR：`cargo build --locked --offline --release --manifest-path tools/research/123/Cargo.toml`；保留生成的pipeline123二进制。它执行cold/unchanged/private_edit/public_edit四个完整请求，每次独立Verify。
3. `python tools/research/146/prepare.py --repo BASE_SOURCE --out NEW_EXPERIMENT`，复用事先登记的22族规模与5/15/60/20权重。脚本输出的旧141标签仅是原始输入资产来源，下一步冻结为146身份。
4. `python tools/research/146/freeze.py --experiment NEW_EXPERIMENT --baseline A_PIPELINE --candidate C_PIPELINE --baseline-source BASE_SOURCE --candidate-source C_SOURCE`。
5. `python tools/research/146/campaign.py --root NEW_EXPERIMENT --baseline A_PIPELINE --candidate C_PIPELINE --mode preflight`。
6. 等当前机器自身构建/测试工作结束后，以同参数 `--mode measure`；固定六块ABBA/BAAB。脚本不会重试失败到通过，不覆盖已有timings。
7. `python tools/research/146/summarize.py NEW_EXPERIMENT`。共同成功覆盖与缺失任务能力单列；报告完整请求期望耗时比，不把失败耗时加入分母。
8. 两侧另构建zil CLI；`python tools/research/146/check_native.py NEW_EXPERIMENT BASE_ZIL CANDIDATE_ZIL NEW_NATIVE_OUTPUT` 检查源码→LLVM→原生、阶段字节和真正命中Verify的负例。

计时不与编译/大量复制并发。CPU affinity仅固定本进程CPU，不声称独占共享宿主。附带旧工具的源码/原始失败说明均保留；最终正式准入仍需独立审核。
