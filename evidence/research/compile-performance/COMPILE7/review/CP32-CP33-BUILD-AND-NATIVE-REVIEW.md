# CP32 / CP33 构建绑定与 CP32 native 作者证据静态复核

结论：**Pass，仅限构建绑定和已保存作者证据的静态核查**。审核 cp25_evidence。本轮没有运行编译器、harness、测试或 native 程序；读取源码、Cargo 元数据、收据、原始 IR/ELF/输出，并重核摘要。

## 冻结构建

| 项目 | CP32 | CP33 |
|---|---|---|
| 精确 source commit | `3ba131efdeffe85ba5fccd1c177745d812a6ce61` | `83854ae83f7fec4fc8f9c22d0751a1fd2725671f` |
| source checkout | `zil-cp32` | `zil-cp33` |
| 绑定源状态 | 构建前后及本次复核 clean | 构建前后及本次复核 clean |
| 受测 harness SHA256 | `5487697b599112159fdfc10a38865d3ebb2cfd4814bb5185be3efde4f5ca5d72` | `81ac94492e5c3065fc1d67c00dae0fc6191d8b157101ab5671ae4a5b4e23b129` |
| 条件标记 | `--cfg cp29` | 无 |

两者都绑定同一冻结 Rust harness 源，SHA256 `3aaf249ad90b67029bacedc66523ce149c230099af27fdc22ed2a6074ee5a37f`，与 baseline 和根工具源码逐字节相同。CP32 的 cfg 只控制 compile_path 调用计时结束后的 memo 统计输出；编译路径、序列及成功/Verify/观察量门禁共享同一实现。全进程统计包含该报告成本。

实际 Cargo JSON 和 fingerprint 均核对：driver/backend 的 feature 为 `[default, llvm-api]`，LLVM API 为 `[default, native]`。同 baseline 的 rustc/profile/compile_kind 一致，rustflags 为空；release opt-level=3、debuginfo=0、debug_assertions=false、overflow_checks=false。链接共同使用 Rust 2024、-O、debuginfo=0、panic=abort 与同一 LLVM 动态库目录。两者所有链接返回码为 0，链接 stdout/stderr 均为空。

本次共核 **59 个文件**的现时摘要，包含所有候选 release rlib、二进制、源码快照、构建日志、工具及 fingerprint。CP33 顶层 driver rlib 与实际 deps rlib 为同一文件。CP32 的 CLI、native 控制二进制也被核回原收据。构建命令与工具日志的耗时不属于性能样本。

机器收据：`review/CP32-CP33-STATIC-BUILD-REVIEW.json`。复核脚本：`review/cp32_cp33_build_static_audit.py`。原构建收据分别位于 `recovery/cp32-release-3ba131ef/BUILD-RECEIPT.json` 与 `recovery/cp33-release-83854ae83/BUILD-RECEIPT.json`。

## CP32 作者 native 控制

CP32 native 控制使用与 CP29 相同的冻结源码，SHA256 `5275c621600ba7a50a52b351a26f1f0a0a12a6a26deff6051f5c2c3abbda037f`，但重新链接 CP32 精确 release 依赖；控制二进制 SHA256 `a49631b2e3db6ac88056fd44d8a5fb59d97b0fca3a006bfe66cc5c41122554f2`。审核已核完原 manifest 的 **113 文件、2,048,723 字节**。

真实源中，main 求 `n=a.get()+b.get()`，b 固定返回 2，a 的私有实现依次返回 1、5、1；随后真实 C putchar 输出 `64+n`，main 返回 n。因此 cold/private/restore 的退出码应为 3/7/3，stdout 应为 C/G/C 单字节。六次作者执行的保存输出、退出码均相符，stderr 全为空。六份 ELF 均为 little-endian ELF64 x86_64。

每对 warm/fresh 的 Typed、Candidate、LIR、LLVM 和 verified-interfaces 原始字节相等，ELF 字节也相等；诊断、公开接口、输入身份和 source_files 一致。cold memo 为 reused=0/checked=6，private/restore 为 reused=2/checked=4，均保留三个纯 scalar 模块。

六份编译报告实际使用 **Clang textual LLVM IR、O0**，不是 LLVM API 原生发射后端。中间故意把 a.get 改成 bool，warm/fresh 在 Types 同样拒绝，Verify=0、artifacts 为空、未发布拒绝路径输出；控制源码断言旧成功 epoch 保留，后续 restore 的结果仍等于 fresh。审核复核了该源码和保存结果，没有重新执行断言或二进制。

**作者 native 执行数为 6；非作者新增 native 执行数为 0。** 此小样本只有三个保留模块，没有跨过 CP32 部分保留容量边界；不能将它称为大样本 partial native 覆盖，也不是性能计量。部分保留边界的独立 Typed 正确性证据另列于 `review/CP32-INDEPENDENT-REVIEW.md`。

原证据在 `recovery/cp32-release-3ba131ef/NATIVE-CONTROL-RECEIPT.json` 及其 `native-control` 目录；静态机器结论为 `review/CP32-NATIVE-STATIC-ARTIFACTS.json`，读取脚本为 `review/cp32_native_static_audit.py`。
