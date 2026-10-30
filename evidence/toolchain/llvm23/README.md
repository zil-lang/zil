# LLVM 23.1.4 升级证据

结论与边界见 [升级报告](../../../docs/toolchain/llvm23/RESULT.md)。
生产实现受测提交为 `48fa354559be0bc7f6330b388a367e30c7184312`；完整 CI 的实际合并检出
`6e673330a68333240d4e09f0ba02f78dd277bc4e` 与其 tree 完全相同。
后续归档提交只保存文档和证据，不将旧运行重新标成新运行。

## 直接入口

- `ci-acceptance.json`：完整 CI，默认 2743/0/0、参考 2711/0/0、显式 text-reference 36/0/0。
- `migration-and-consumer.json`：10 次真实迁移构建/执行，两个缓存配置与两个错配拒绝控制；
  consumer208 的 16 次构建、128 次原生执行和 56 次真实分配失败注入。
- `acceptance-addendum.json`：完整验收证据独立复核 Pass；支持收据为 `acceptance-local-checks.json` 与 `ci-crosscheck.json`。
- `independent-review.json`：非作者完整静态审查及 58 项实际限定测试，两个安装器问题均关闭。
- `source-invariants.json`：核心语言/安全核/IR、规范、产品门和 Cargo.lock 未改。
- `bundle-index.json`：各压缩包字节数、SHA-256 和核验范围。
- `native-archive-retention.json`：三个大型原生产物归档的真实位置、摘要与未完成的额外持久保存状态。

## 压缩包

| 文件 | 内容 |
| --- | --- |
| `source-audit-bundle.tar.xz` | 精确 LLVM tag 的 C API/枚举/所有权源码核查及原始来源 |
| `shlib-audit-bundle.tar.xz` | 上游共享库配方、PIC 组件与实际 ELF/符号审查 |
| `candidate-review-bundle.tar.xz` | 旧 RequestChanges、失败的零选择调用、实际 58 项回归、安装器修复复核 |
| `acquisition-bundle.tar.xz` | 官方发布/摘要、Rust、Ubuntu17 签名链、依赖清单、基线构建与实际失败记录 |
| `installer-validation.tar.xz` | 本地真实安装收据和日志，首次 uid 映射失败及后续成功 |
| `local-validation.tar.xz` | 完整本地 gate 日志、真实迁移控制/消费者原始产物、两次参考模式中断与本地执行脚本 |
| `github-ci.tar.xz` | 完整 CI 日志/收据、真实安装日志、checkout、完整源文件列表及 artifact 摘要；不重复存入整个 Git bundle |
| `native-archive-manifests.tar.xz` | 三个大型 native 包的逐文件摘要和本地保留状态；不是三个大包本身 |

恢复时将各包分别解压到新目录，保留原文件名。原日志的绝对路径只是当次来源记录，
不是新环境的主树地址。所有新的归档成员均与原件逐字比较；来源代理生成的四个审计包
保留其原始摘要及各自核验收据。`LLVM-LICENSE.TXT` 随缓存的 LLVM 上游源码一同保留。

本地完整默认全仓 Pass、首次缺夹具 Fail、参考 ENOSPC、第二次未知原因会话中断分别存放；
后两次均不是参考模式 Pass。参考模式接受依据为完整 CI 的 2711/0/0 及精确 tree 等价。
版本函数 interposition 控制不冒充在完整 LLVM17 ABI 上执行候选。

完整大包仍仅在当前 sandbox。Library 上传被自动审批拒绝后没有重试或绕过；
本目录的清单不能替代缺失的外部大包字节。小型验收日志、控制产物和独立审查已随源码归档。
性能比较、原生 Windows 和全产品验收均未由本单宣布通过。
