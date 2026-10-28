# 204 真实消费者与原始失败记录

五模块源码位于 `examples/mainline/store_captures204/`；独立 runner 和文本证据收集器位于 `tools/language/204/`。这是一份新消费者，203 原源码、oracle 和结果均不改写。

限定范围包括 Fn 的实际拥有子 Closure、FnMut 捕获完整含 Store Package、FnOnce 实际 TakeCapture、两同型内层 Store 不同拥有值、外层 State/Store、普通完整 T helper、元素窗口、0/1/continue/break/满 push/pop、give 前后申请失败清理。另有短源 `impl204_analyze_capture` 用 fallible 参数检查 callee 已准备但 body 未进时的 Store 环境责任，与已 give 后局部清理作可观察对照。

当前仅提交源码和执行前 oracle。编译、Linux/Wine、申请失败矩阵尚未执行，未将计划数记为通过。成功或失败都按实际冻结 source/CLI/命令独立追加；任何新生产变更必须换匹配 CLI，不能倒标旧运行。

## CLI05 实际作者矩阵

上述段落保留源码首次提交时的 NotRun 身份。随后冻结 clean CLI 来源 `7abbbac1f6479c52f4967653acfab74fc56c945d`、SHA `baa0b5757871b5f7bc31713f61c0a91042d1396087d9bddc65d6872c82a4312d`；消费者 checkout `7bf69fe193d0e6f6bcfc62e9d1fa955afb26c61c` 的生产输入与其逐字匹配。

Linux01 与 Wine02 各完成 148 次真实执行（16 普通、132 allocator probe），各 108 次实际申请失败，全部符合原预期；O0/O2 各一份五模块构建，Verify 各 1。所有 probe 最终 live=0，Wine 同时 bad_operations=0。每平台活跃申请的 payload 峰值为 528 字节、4 个分配；这些是该矩阵申请值，不是 RSS 或正式性能结论。

Wine01 首次尝试在编译 Win64 PE 后被沙盒本机 IPC 限制挡住，`wineserver: bind: Operation not permitted`，零个成功观察；原失败保持。Wine02 用相同代码、CLI、oracle 和既有前缀，在允许本机 IPC 的进程环境实际执行。原生 Windows 不据此升级。

短源实际 callee 准备失败对照同样通过：参数成功为 `PCXATZ`/exit0，参数 `?` 失败为 `PTAZ`/exit1，O0/O2 各自执行。新 Store 窗口反例另在两核修复后重跑；合法最后使用、不同 capture/不同 Package 字段根保留，原坏源在 Analyze 明确拒绝。小收据在 `evidence/impl/204/analyze/RESULT.json` 与 `evidence/impl/204/consumer/RESULT.json`，完整文本/Wire 原件待独立封包，不收入源码包的机器产物。

## Rust 四目标与突破判断

| 目标 | 本片实证 | 未建立的结论 |
|---|---|---|
| 同类安全 | 两核独立重建叶资格、真实捕获来源与窗口失效；原误接受输入以及最近合法字段/最后使用对照均保留 | 有限 profile 的证据不等于完整语言安全证明 |
| 运行与内存 | 双平台各 148/108、清理标记与最终 live=0，实际申请 payload 峰值 528B | 未做同语义 Rust 运行时间、RSS 或尾延迟配对 |
| 表达与维护 | 普通五模块库能把实际拥有 Store 的 Fn/FnMut/FnOnce 放入 State/外层 Store，随后在真实位置调用和转交 | 未完成完整 ZUI 框架或维护工作量比较 |
| 编译成本 | 原普通 Store Analyze 561、原 ByteBuffer 捕获 2230 保持；新实际窗口 12964 有 N/N−1/fresh 证据 | 不把逻辑预算、限定测试时间或本片新增能力当编译速度胜出 |
| 实质突破 | 原关闭的拥有捕获与持久 State/Store 组合已形成可重放的机器及失败清理闭环 | 产品级四目标合取与相对 Rust 的净突破未建立；原 product-gates 不变 |

完整 1.4、产品和原生 Windows 仍按各自未完成状态报告；最终整仓及切片验收由集成与非作者收口，作者矩阵不替代它们。
