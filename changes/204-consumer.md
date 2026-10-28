# 204 真实消费者与原始失败记录

五模块源码位于 `examples/mainline/store_captures204/`；独立 runner 和文本证据收集器位于 `tools/language/204/`。这是一份新消费者，203 原源码、oracle 和结果均不改写。

限定范围包括 Fn 的实际拥有子 Closure、FnMut 捕获完整含 Store Package、FnOnce 实际 TakeCapture、两同型内层 Store 不同拥有值、外层 State/Store、普通完整 T helper、元素窗口、0/1/continue/break/满 push/pop、give 前后申请失败清理。另有短源 `impl204_analyze_capture` 用 fallible 参数检查 callee 已准备但 body 未进时的 Store 环境责任，与已 give 后局部清理作可观察对照。

当前仅提交源码和执行前 oracle。编译、Linux/Wine、申请失败矩阵尚未执行，未将计划数记为通过。成功或失败都按实际冻结 source/CLI/命令独立追加；任何新生产变更必须换匹配 CLI，不能倒标旧运行。

## CLI05 实际作者矩阵

上述段落保留源码首次提交时的 NotRun 身份。随后冻结 clean CLI 来源 `7abbbac1f6479c52f4967653acfab74fc56c945d`、SHA `baa0b5757871b5f7bc31713f61c0a91042d1396087d9bddc65d6872c82a4312d`；消费者 checkout `7bf69fe193d0e6f6bcfc62e9d1fa955afb26c61c` 的生产输入与其逐字匹配。

Linux01 与 Wine02 各完成 148 次真实执行（16 普通、132 allocator probe），各 108 次实际申请失败，全部符合原预期；O0/O2 各一份五模块构建，Verify 各 1。所有 probe 最终 live=0，Wine 同时 bad_operations=0。每平台活跃申请的 payload 峰值为 528 字节、4 个分配；这些是该矩阵申请值，不是 RSS 或正式性能结论。

Wine01 首次尝试在编译 Win64 PE 后被沙盒本机 IPC 限制挡住，`wineserver: bind: Operation not permitted`，零个成功观察；原失败保持。Wine02 用相同代码、CLI、oracle 和既有前缀，在允许本机 IPC 的进程环境实际执行。原生 Windows 不据此升级。

短源实际 callee 准备失败对照同样通过：参数成功为 `PCXATZ`/exit0，参数 `?` 失败为 `PTAZ`/exit1，O0/O2 各自执行。新 Store 窗口反例另在两核修复后重跑；合法最后使用、不同 capture/不同 Package 字段根保留，原坏源在 Analyze 明确拒绝。小收据在 `evidence/impl/204/analyze/RESULT.json` 与 `evidence/impl/204/consumer/RESULT.json`，完整文本/Wire 原件待独立封包，不收入源码包的机器产物。
