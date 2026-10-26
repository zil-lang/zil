# Roam 开发基础

本目录保存用户附件的原始 TS Roam，并把真实长驻 `CompilationSession` 接入原状态路径执行、进程监督、BFS 和缩减器。它是编译器开发基础，不增加语言语法、程序权限或生产依赖。独立源码语义 oracle 由 `tests/oracle/` 提供；本适配器负责会话缓存透明性。

## Rust 原生入口

当前开发首选 `tools/roam-rs/` 的独立 Rust 会话 profile；其 `README.md` 给出离线构建、六态协议、相同状态路径、历史重放与边界。它不依赖 Node、Bash 或 TSV 桥运行正常 campaign。本目录原始 TS 和适配器继续用于协议对拍及旧证据重放，vendor 未修改。

## 运行

环境为 Linux、Node 24.19.0、Bash、GNU cat，以及仓库固定的 Rust 工具链。Node 的 `--experimental-transform-types` 只服务测试宿主，因为原件包含参数属性等非 erasable TypeScript。原源码没有改写为另一个测试框架，也不依赖 Bun 运行本适配器。

在仓库根目录、加载 Rust 工具链后运行：

```sh
node --experimental-transform-types --import ./tools/roam/register.mjs ./tests/roam/run.mjs
```

这只编译`zil-driver`的`roam_session` example，并运行本责任域的有限门禁，不运行全工作区测试。默认生成新的`out/runs/roam-ts/<RunId>/`，包含时间和随机后缀；`--out=/绝对路径/新目录`仍可指定位置，已有目录拒绝覆盖。`--binary=/绝对路径/桥接程序`可使用已编译程序，并记录确切二进制摘要；此模式不证明其源码与提交的对应关系。正式门禁仍要求已跟踪文件干净，编译源码和适配器目录没有未提交的新文件；开发跑测可明确加`--allow-dirty`，报告保留该标记。输出迁移不改变状态路径、计数或失败分类；验收时晋升小收据，raw按RunId选择归档。

完整 JSON 是 `report.json`。退出 0 只代表声明范围内所有正常路径通过，同时故障注入确实被发现、重放、缩减、干净对照通过。任何缺失结果、超时、协议错误、未支持操作、预算耗尽都不会充当正常路径通过。

## 接入契约

外层保持原 `roam/1` case 与 `roam-worker/1` JSONL 协议。`initial` 可省略或等于默认状态；`steps` 中的具体操作是历史重放权威。seed 记录生成来源，历史重放不会用 seed 重新生成路径。

| 操作 | 参数 | 检查的责任 |
| --- | --- | --- |
| `edit` | `zero`、`one`、`ct`、`type-error`、`syntax-error` | 有效、无效源码之间的会话恢复 |
| `check` | 无 | 同一源码和配置下，长驻会话与全新会话比较 |
| `target` | `native-llvm`、`alternate-llvm` | 改变显式目标身份；不代表跨架构本地生成覆盖 |
| `cancel` | `none`、`before`、`parse`、`types`、`verify` | 取消、活动查询回收以及后续恢复 |
| `limit` | `tokens`、`nodes`、`analysis`；0 至 10,000,000 | 配额改变后的热、冷行为一致性 |
| `cache` | `on`、`off`、`clear`、`corrupt` | 原始查询缓存与磁盘源码缓存的恢复 |

桥接程序使用真实 `CompilationSession::compile`，每次检查新建单独的冷会话，且冷会话禁用磁盘缓存。双方各使用新的取消令牌，避免把暖运行的取消状态偷渡到冷对照。比较项目是状态、阶段、诊断及输入指纹；查询执行和缓存重用统计可以不同，但 Verify 必须执行，不能重用。每个步骤检查活动查询为零。真实编译桥的内部命令仅是有长度界限的 TSV；外部交付的 case、状态、报告和监督协议保持 JSON。

## 原件与宿主边界

`vendor/` 是附件中的全部 49 个普通文件，逐字节保留，没有筛除插件。`vendor-manifest.json` 保存原压缩包摘要、逐文件 SHA-256 和大小。运行前验证完整文件集合与摘要。仅限本项目授权的开发使用；附件没有在这里被重新授权公开分发。

本适配器实际调用原 `execute`、`serveWorker`、`supervise`、`FileStore`、`bfs`、`shrink`，固定路径、枚举路径、seed 路径以及缩减候选都经过真实的原 supervisor。原 worker 在每一步前发送动作，supervisor 持久化检查点后 ACK，再执行实际编译；动作完成后发送 `after`。失败结果保留原件 case、环境、结果、检查点和 stderr。

当前沙箱直接由 Node 启动另一个 Node 时，socketpair 标准流曾导致工作进程退出 0 却没有任何协议消息。原 supervisor 正确把它判作 `missing-result`。门禁保留一个直接宿主探测及其失败证据，此探测单列为环境能力诊断，不计作编译器通过。实际门禁显式采用 `pipe-host.sh` 的真实 POSIX 管道，逐字节转发协议；`"$@"` 保留参数数组，`pipefail` 传播工作进程错误。原 supervisor 仍管理超时、进程组和结果完整性。没有修改 vendor 或容忍丢失消息。此适配暂限 Linux，不能据此宣称全部宿主平台可用。

原 Roam 顶层宿主与依赖 `Bun.build` 的 `freezeDriver` 没有在本适配器中接入。`freeze.mjs` 是明确标识的 ZIL 适配层：复制原件、适配器源码和实际桥接可执行文件，保存每个文件摘要；同时钉住 Node、Bash、cat 的版本或可执行文件摘要。历史重放先验证文件集合、每个摘要、运行时身份，再执行冻结的 worker 和原 supervisor。OS、动态库、CPU 环境没有形成容器级冻结，因此不承诺跨环境位级复现。

## 覆盖范围与反例

有限门禁包含 11 条明确风险路径、七动作字母表深度不超过二的 56 条边，以及 16 个固定 seed 产生的各 64 个随机动作和 7 个恢复动作。原 BFS 以完整路径作为键，不把源码相同但缓存历史不同的会话合并。报告保留 `complete: false` 和 `stopReason: depth`；这是已声明短路径集合的覆盖，不是整个状态空间穷尽。

注入只发生在测试桥：源码改变后故意返还上次编译报告。三动作字母表下 BFS 应找到三步见证，再把带冗余步骤的八步原例交给原缩减器。要求原例冻结重放三次、缩减例再次失败、关闭注入的相同缩减路径通过。原例文件不能被缩减覆盖。该注入是检验测试基础能发现问题的阳性对照，不能作为生产编译器缺陷来统计。

原缩减器保留同一 `propertyId` 和有效路径，并逐块、逐步删减；这不证明同一根因，也不证明任意操作集合下全局最短。BFS 的最短声明只适用于所声明的有限动作集合和观察条件。本适配器的热、冷两侧共享编译器实现，因此不能发现双方共同的语义误编译；独立 BigInt 源码 oracle 承担另一层责任。

## 历史重放

从 `report.json` 的 `injection.evidenceId` 和 `injection.reduced` 取值：

```sh
node --experimental-transform-types --import ./tools/roam/register.mjs ./tools/roam/replay.mjs <证据目录> <evidence-id>
node --experimental-transform-types --import ./tools/roam/register.mjs ./tools/roam/replay.mjs <证据目录> <evidence-id> --reduced=<reduced-ID.json>
node --experimental-transform-types --import ./tools/roam/register.mjs ./tools/roam/replay.mjs <证据目录> <evidence-id> --reduced=<reduced-ID.json> --clean
```

前两项复现注入失败时退出 1，干净对照应退出 0，基础设施问题退出 2。不能只把“命令非零”理解成未成功复现；应检查 `status` 与 `failure.propertyId`。重放读取原证据，不覆盖 case、缩减件或初次结果。继续施工时，应同时保留源码提交、工具链、整个证据目录及对应快照；只保存 seed 或缩减路径会失去原失败上下文。
