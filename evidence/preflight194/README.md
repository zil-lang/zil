# 194 只读预检

本目录记录候选 193 CLI 对 19 条源程序的真实观察，不包含 194 实现或原生执行结果。详细根因、主树规范快照、建议写集与身份分析见 `changes/194-proposal.md`。

- CLI source：`f594c57665aaf34d0c1d54df11d5047d4d042f0d`，原构建状态 clean。
- CLI SHA256：`3830f924b7fa1223aeec88ecfd406f0a3012095ebd5be42a10aae0a7357ae33e`。
- 观察：19 条，Success 3、Unsupported 15、SafeReject 1；verification_runs 合计 3，native_executed 0。
- 每条请求 `--stop-after verify`，实际 stop_phase 和完整诊断见 `SUMMARY.json` 及逐项 `stdout.json`。前端拒绝不算 Verify 已执行。
- `IDENTITY.json` 分列所读 main 和 CLI 源码身份/哈希，明确两者不是同一个提交。
- `run.py` 拒绝覆盖已有 SUMMARY/案例目录。复现时应在新的输出目录复制脚本，再指定可核实的相同 CLI。
- 构建收据仅用于绑定现有 CLI；没有重复构建或创建 target。本目录没有二进制/目标对象/可执行产物。

首次源码风险分析中的“AST 克隆保留 NodeId”已被真实 lowering 路径否定；提案明确记录该修正。未观察到被接受的 mixed-generic 程序发生碰撞，因为目前它们均在 Expand 准入门被拒绝。
