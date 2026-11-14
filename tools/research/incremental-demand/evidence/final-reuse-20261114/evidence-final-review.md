# Final reuse chain 13：展示修正最终复核

**ACCEPT。初审两处展示问题均已闭合，无剩余问题。** 本次仅复核两处修正和绑定；不覆盖初审原文，不重跑构建、测量或 archive 恢复，不修改 tracked 文件。

绑定的编译／测量 source 仍为 `8ce584f81a405b8c5aa69e555b80ede276f07143`。

- JSON SHA256：`90425ba6c99ebe710c8fe5b105c46d99955ca1fcf57df0a9abf30e1bda208dbf`。
- Markdown SHA256：`01a84e6afe10f92e80949f552413b0dd994336191a385fc34390de3ba5b5b929`。

原反向字段已删除，`candidate_leading_over_Rust_median_and_paired_GM_cells` 正确列出候选中位数和 paired GM 均短于 Rust 的 18 格。negative_observations 完整对应原分析中的两条逐 block 负观察：check/broad 的 `slower_than_main_blocks=[1]`，O2/recovery 的 `slower_than_Rust_blocks=[1]`；相应原样本／中位数／GM 保留，Markdown 明示两条数字和相对范围。

实读 `display-correction.json` 的新 JSON／MD hash、前版 outer JSON hash `d218cff936beb34539ecdc5a18ff8c77770632da5d045525d9008436e4b6f290` 均与本次及初审记录一致。archive 旁 `evidence-initial-review.md` 与原初审文件完全同 hash：`33d1aa21155c5b77905bbedeac7dad280aee6fdffb2a32b9dc7e60b045b6b4af`。旁附 corrected synthesis 收尾实读 SHA256 为 `32f678b798139c0dc0334cc494c445e983025891ca8432b9c4e84dc524cf9909`，与更新后的 correction 记录相符；已核末尾 stdout 进度标签为 `candidate_leading_over_Rust_cells`。本次实读 `display-correction.json` SHA256 为 `625235c9f0d06076e707af58b53a715fd901d2a336aaac546a6b4aa5fa8d1b53`。该收尾更新仅补记脚本／correction 字节绑定，ACCEPT 裁决维持。

当前 outer 的 cells、raw hashes、completion 与原 strict analysis 一致；strict analysis SHA256 仍为 `55834c439b688a0b6ba9193a1c7cc28096f4bb1e85946013b53b9bb8a59dc38d`。sealed archive 绑定仍为 `3e00d02563d2ffa34dc20defd8f9391c21d8bb31bd8935f1595cb12002f6c725`，没有以展示修正替换历史 raw snapshot。两 block 的有限窗口结论和不跨 b6 归因的边界保持。

root 可据此提交／推送证据；新的证据 commit 及远端 readback 由 root 随后确认，本报告不提前宣称已发布。
