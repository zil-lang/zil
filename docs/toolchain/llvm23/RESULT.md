# LLVM 23.1.4 升级结果

2026-10-30（Asia/Shanghai）。一次性维护分支：`chore/llvm-23-upgrade`。
[Issue #6](https://github.com/zil-lang/zil/issues/6)；[PR #7](https://github.com/zil-lang/zil/pull/7)。

**升级实现和现有 Linux 验收门已完成，主线合入待处理；编译性能收益尚未测量。**
原 main 和编译专项都仍使用 LLVM 17.0.6，因此这次确实补上了未完成的升级。
本分支从当时最新 main 创建，未携带长期编译专项的 CP02 Hold 候选。

## 精确受测身份

| 项目 | 身份 |
| --- | --- |
| main 基线 | `ad509b5003d9d2a303428f9d01732292988084d8` |
| 升级实现提交 | `48fa354559be0bc7f6330b388a367e30c7184312` |
| 实现源码树 | `0a5ce57b74581e55b34be381ca7f7b54f1ed921a` |
| crates 子树 | `00f82b6884c647269c76de05340cc4f82b98b0ea` |
| CI 实际 PR 合并检出 | `6e673330a68333240d4e09f0ba02f78dd277bc4e` |
| 完整 CI | [40921888880](https://github.com/zil-lang/zil/actions/runs/40921888880)，Success |

CI 合并检出的两个父提交恰为上述 main 与实现提交，tree 与实现 tree 完全相同；
完整 `git ls-tree -r` 输出也逐字一致。后续报告归档提交不更改生产源码、工具链入口或工作流；
它不改变这次实际运行的提交身份，也不把旧运行伪称为归档提交上的新运行。

## 已实施的迁移

LLVM、Clang、LLD 从 17.0.6 固定到官方稳定版 **23.1.4**。
`tools/llvm/version.txt` 是统一 pin；Rust 构建脚本据此生成版本常量和链接主版本，
原生入口通过真实 `LLVMGetVersion` 严格拒绝错版，对象收据使用同一版本身份。
checker 核对实际 LLVM DSO、Clang、LLD、Rust、Cargo 的版本，并记录路径和摘要。

官方 Linux X64 发布包没有可直接使用的 libLLVM 共享库。安装器验证完整官方归档的
大小和 SHA-256，确认 PIC 配置，再按上游 llvm-shlib 的 whole-archive、符号版本、
SONAME 与链接约束，从 61 个组件链接所需 X86 DSO。收据明确标记“从官方组件本地链接”，
列出组件与系统运行库身份；没有将它标成官方预编译 DSO。
当时 apt.llvm.org 的 23 分支已是尚未稳定发布的 23.1.5 快照，未用其替代稳定目标。

独立上游审计覆盖 99 个直接 C 声明、5 个 X86 初始化符号、9 组枚举中的 21 个常量，
以及属性与所有权约定；全部 104 个所需符号在真实 DSO 中可见。
`LLVMParseIRInContext` 消费输入 buffer 的既有所有权处理不变。
FFI 声明、语言语义、Verify、优化 pipeline、target/CPU、默认 CGU 策略和对象缓存协议均未改动。

### 依赖核查

| 依赖范围 | 本次处理 |
| --- | --- |
| LLVM / Clang / LLD | 升至并固定 23.1.4，迁移当前安装、检查、运行时与 CI 入口 |
| Rust / Cargo | 1.99.1 当时已是稳定最新版，保持现有 pin |
| 核心 Cargo workspace | 15 个本地包，零 registry/git 外部包；根 Cargo.lock 不变 |
| 独立 Roam 工具 | serde_json 1.0.154 及全部 11 个 registry 锁定项核对后无需更新 |
| 可选 Playwright | 1.64.1 核对后无需更新 |
| GitHub Actions | checkout v7.0.1、upload-artifact v7.0.2，完整 SHA 固定，独立维护提交 |

Rust 自带的 LLVM 23.1.1 与 ZIL 使用的 LLVM 是两条工具链，前者的版本不能证明后者已升级。
系统运行库按实际宿主记录，没有声称所有 Linux 系统包都是跨发行版的“最新版”。
历史 LLVM 17 实验、脚本和收据保持原版本，以保留原实验身份。

## 实际验证

以下是不同运行的范围；重叠用例不相加，不用测试数量代替产品结论。

| 检查 | 结果 | 运行证据 |
| --- | --- | --- |
| 格式 | Pass | 本地；完整 CI |
| 默认 workspace | 2743 passed / 0 failed / 0 ignored | clean 48fa354 本地；同 tree 的完整 CI |
| 无默认 feature 的参考 workspace | 2711 / 0 / 0 | 完整 CI；本地两次未完成另列 |
| release CLI | Pass | 本地冻结候选；完整 CI |
| 真实 consumer208 | 16 builds、128 native runs、56 次真实分配失败注入，Pass | 本地与 CI 分别运行 |
| all-target Clippy，`-D warnings` | Pass | clean 48fa354 本地；完整 CI |
| 显式 LLVM text-reference | 36 / 0 / 0 | 本地、独立复核和 CI 分别运行 |
| 迁移 runner 的合成测试 | 9 Pass | 本地与 CI；不替代下面的真实迁移控制 |
| 非作者限定回归 | 58 / 0 / 0 | 36 项叶子测试，加 22 项 driver/cache 测试 |

consumer208 的本地 40 个对象收据均报告 LLVM 23.1.4、Builder、`parse_ns = 0`、
LLVM 前后核验；所有 16 次构建都执行当前独立 ZIL Verify 一次。

### 17 / 23 共存与错误版本拒绝

用冻结的两版真实 ZIL CLI、真实 Clang17/23 与真实 LLVM17/23 执行 10 次构建、10 次原生程序。
每个配置依次执行 `17 cold → 17 warm → 23 cold → 17 warm → 23 warm`，
共享同一源文件、输出位置及对象缓存目录。

| 配置 | 17 cold | 17 warm | 23 首次 | 回到 17 | 23 warm |
| --- | --- | --- | --- | --- | --- |
| O0，请求 4 CGU，实际 3 个单元 | 3 miss | 3 hit | **3 miss** | 3 hit | 3 hit |
| O2，实际 1 个单元 | 1 miss | 1 hit | **1 miss** | 1 hit | 1 hit |

所有原生程序均返回 42；每次请求仍执行当前 Verify。23 没有错误复用 17 的对象，
已有 17 条目也未被覆盖；无需删除旧缓存才能升级。

真实 Clang17 配合 LLVM23 被 checker 拒绝。另一个明确标作“版本函数 interposition”的
控制使 `LLVMGetVersion` 报告 17，候选返回 `E-LLVM-API-VERSION`，保留原可执行文件、
`.ll` 与缓存字节。该控制验证拒绝路径，不声称在完整 LLVM17 ABI 上运行了 LLVM23 候选。

## 失败记录与修复

所有原始失败均保留，没有用后续 Pass 覆盖。

- 首次安装在受限 uid 映射下不能恢复归档原属主；安装器增加 `--no-same-owner`
  与 `--no-same-permissions`，之后真实安装成功。
- 独立审查发现 `-lz` 需要开发包的未版本化链接，而前置只声明运行时包。
  48fa354 改为 `-l:libz.so.1`，并补记直接列出的 ELF loader；独立复核关闭这两项。
- 首次默认全仓在稀疏检出中缺少 benches109 夹具，出现 5 个失败。
  恢复同一 Git 提交内已有输入后，重新运行得到 2743/0/0；没有改变生产源码来消除失败。
- 独立 reviewer 第一次 driver 命令漏了 `--features llvm-api`，选择零个测试。
  runner 正确判 Fail，修正命令后实际执行 22 项通过；零执行不计入通过数量。
- 本地参考模式首次因 ENOSPC 中断，连退出结果序列化也失败；Cargo 退出码记 unknown。
  归档后重试出现执行会话消失、进程不再存在且没有最终收据的中断，确切终止原因未知。
  两次均未判 Pass。完整参考模式验收来自精确同 tree 的 CI，2711/0/0。
- 较早 CI 40921240770 被新提交触发的并发策略取消；它不计作通过。

## 来源、证据及复现范围

完整小型证据位于 [`evidence/toolchain/llvm23`](../../../evidence/toolchain/llvm23)：
上游 API/共享库审计、获取与签名/摘要记录、真实安装日志、完整 gate 输出、迁移控制与
消费者原始输出、独立审查、GitHub CI 原始日志和精确 checkout。
压缩包索引记录每包字节数和 SHA-256；新增归档每个成员均已逐字核对。

官方 SDK 归档 SHA-256：
`efdd1dbd089b99bfcfcde7b8eb1236163f2e88df5a981cca4f783ec9cb5e0378`。
本地 DSO：`4af7bc67bebd262ac4944cbd5a7996bdae1479cf61ee625ab1d98f44d1ceea75`；
CI DSO：`82441dae7d94aac9e1964b06ee6d4bb0f28c0fcf3aaa31df91c886806a112567`。
两者来自相同官方组件，在各自宿主链接、核验并通过测试；宿主链接器与系统库身份单列，
不声称跨宿主 DSO 字节可复现。官方 GitHub attestation 签名验证为 NotRun，不能以 SHA 校验代替。

CI Rust/Cargo 路径摘要指向实际调用的 rustup dispatcher，版本命令报告 1.99.1。
该摘要不等同于分派后 rustc/cargo 文件的哈希。本地独立安装使用已核对官方摘要的实际组件。
本轮没有以 CI shim 哈希建立编译性能配对结论。

三个大型 native 原始归档及逐文件清单在 sandbox 中保留，归档均已逐字验证。
其额外 Library 上传被自动审批拒绝，理由是私有项目派生产物缺少该目的地的明确授权；
没有上传或绕过。Git 保存完整小型验收日志、控制证据及大包摘要/清单，
**不把仅存在于当前 sandbox 的大型原始包声称为已持久保存**。
GitHub Actions 原始 artifact 另有 30 天保留期，完整小型证据已随本分支归档。

规范文本与 product-gates 原字节不变。只有 `policy/dependencies.json` 的上游版本记录改变规范输入摘要：
`4d5480fd3f7de30f6a846489e624ce01bb64d7e835c7e01930bd3410a2206db1`
→ `8b4b3d219999c8f2904b276e051745bf65f9761e421630bdb4bbe0ec2781e02d`。
spec_hash 仍为 `245cf29a95d721da6268c43191204777a1015639cba56e8e9025aeac012d0a2c`。
历史工单与收据不回填；合入时由集成者登记在途工作的工具链输入迁移。
既存历史 handoff/恢复资料缺口不由本次升级修复，本报告不宣布全项目 handoff Pass。

## Rust 四目标与下一步

| 目标 | 本轮证据与判断 |
| --- | --- |
| Rust 同类安全 | 保持当前独立 Verify、版本错配拒绝和已实现语言片的回归；未证明完整语言安全 |
| 运行与内存同级 | 真实消费者、分配失败与清理通过；没有同语义 Rust 时间/RSS 对照 |
| 表达与维护更简单 | 版本 pin、安装与检查入口统一；语言表达和实际维护成本优势未测 |
| 编译成本更低 | 完成工具链迁移基础；LLVM17/23 性能比较 NotMeasured |
| 至少一项实质突破 | 本轮是兼容性维护；没有新增产品级突破或四目标合取结论 |

完整语言仍 NotSatisfied，产品与原生 Windows 验收 NotRun，既有 B0/产品门状态保持。
Linux 上的 Win64 目标/对象检查不能替代 Windows 本机执行。

建议独立审阅并合入本 PR；一次性 `chore/llvm-23-upgrade` 随后可删除。
长期 `research/compile-performance` 吸收新 main，冻结 LLVM23 基线，再进行同源17/23
编译 wall/CPU、分阶段归因、对象体积及目标程序运行质量的配对测量。
CP02 默认关闭成本仍为 Hold，既有 LLVM17 的 CP01/CP04 负结果保留原身份。
后续优化按独立小 PR 进入 main，研究分支持续保留；不把正确性 Pass 当作性能 Go。

官方来源与实施边界见 [PLAN.md](PLAN.md)。
