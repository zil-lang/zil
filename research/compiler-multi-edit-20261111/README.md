# 编译效率专项：多点修改与并发候选（2026-11-11）

本轮基线为 `main@99035931d358add0eaed6d4535035c38413701dd`。两个候选均未采用；本目录保留实现、测量器、正负结果和独审，不代表生产提速已经交付。一般编译效率和全场景 Rust 对比目标仍开放。

## 结果与入口

- [完整结果和实施路径反思](multi-edit/continuation-results.md)：a60 新多文件矩阵假设加权仅省 0.9475%，24 格 12 快 12 慢；1679 八格真实运行退化 61.6%–112.3%，均触发原 10% 门。
- [主线并发独审](multi-edit/main-concurrency-result-independent-audit.md)：同一 main 二进制的 A/A 控制加 Rust；总四 CPU、256 文件、每候选改 16/64 文件、并发 1/2/4。252 编译、756 I/O 通过，六格 Rust 编译均更快；不是优化候选收益。
- [源码保全](multi-edit/source-preservation.md)：远端 a60 与 1679 作为归档提交父对象保留；作者本地提交与发布提交同 tree，不冒称同 commit。精确补丁及最终源码独审同目录保留。
- [原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)：`zil-multi-edit-raw-evidence-20261111.tar.gz`，版本 1，SHA256 `6058113ae324218f7e159c7f9fc35be71408e7bebf5210ce79d00554863fe330`；6212 份原始记录、10 个关键产物路径按 SHA 去重为 8 份实体。[完整索引](raw-evidence.json)。
- [缩小导入的机器码归因](bounded-imports/attribution-1679/report.md)、[available_externally 历史反证](multi-edit/available-imports-history-audit.md)、[退役顺序和原保护条件](retirement-multiedit/README.md)。

## 已执行与未执行

| 范围 | 实际状态 |
| --- | --- |
| a60 原 84 请求 screen | 完整执行，结果混合，长尾与负面值保留 |
| a60 新 after 多文件矩阵 | 378 编译、1134 I/O；未建立可采纳净收益 |
| 1679 native 运行门 | 24 clean 编译、216 native（144 正式），全部功能正确但八格性能 NoGo |
| main990 A/A + Rust 并发 | 36 最终批次、84 最终子进程；含 clean/seed 共 252 编译、756 I/O；独审完成 |
| a60 已准备的 off / 候选并发 | 未执行 |
| 1679 后续编译 / 候选并发 | runtime NoGo 后未准备、未执行 |
| 两候选完整产品 CI / 原八项综合验收 | 未宣称已执行或通过 |

冻结原始包中的 `index.json` 与 `raw-manifest.json` 给出全部原路径、逐项 SHA 与范围。仅指定 n64/base、n256/base 的 A/B/R 六个 ELF 及 A/B 四个 IR 额外保存，未声称包含全部生成产物；seed 文件覆盖的限制见独审。生成工作负载由本目录固定脚本和包内 protocol 重建并核对源文件哈希。历史报告的原相对 JSON/日志链接指向包内 `raw/<同目录>`，不表示这些原始文件又逐份进入 Git。

## 保全与清理

旧 `task/compiler-composition-validation` 已先保全并删除，收据见原始包 `raw/multi-edit/retirement-receipt.json`。本目录初次归档时 a60 / 1679 实验 ref 尚在；后续只在精确源码、证据和自身退役 recipe 已可达固定归档后顺序删除，并另存完成收据。清理状态以最后的 retirement receipt 为准。

当前环境中旧会话工作目录未挂载，已恢复 LLVM/Clang/LLD 23.1.4 和 Rust 1.99.1；随后复用此环境及独立目标目录中的依赖。完整 O2、每请求 Verify、当前机器预检与原运行质量门保持。


## 最终退役结果

两条本轮实验ref已按原guard顺序退役：layout运行[42988640504](https://github.com/zil-lang/zil/actions/runs/42988640504)、bounded运行[42988998893](https://github.com/zil-lang/zil/actions/runs/42988998893)均成功，精确ref删除后404。源码/证据初次保全提交为 `a48374cce53d1018fe52a1c749990db4248f7135`，直接保全recipe的归档为 `a018e25431dc74a5d56e7e2df9ecf75b9f179717` 和 `5c97386cef5a337c432fc7f1351dfcc22a0c02e6`。旧composition也已完成退役。实际完成收据与[最终独立回读](retirement-multiedit/retirement-final-readback.md)在本目录保留；先前草案和首末尾LF差异/修正记录均保留。主线没有新合入的优化，广泛Rust目标保持开放。
