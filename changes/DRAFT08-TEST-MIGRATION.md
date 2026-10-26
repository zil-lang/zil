# draft08 旧夹具迁移

## 先行提案

基线为 root 指定的干净主树 `4733f62`。统一门
`out/runs/validation/20261023T105102Z-2ddcac5a/rust-tests/` 保留了三个旧夹具失败；
本变更只校准这些测试对已合入 042/043 的预期，不修改生产代码或放宽语言边界。

1. `zil-analyze/tests/budgets.rs` 的三个原始类型 metadata 位置继续要求在哈希前拒绝
   未实现类型。原输入一层 `&mut i32` 已受 042 支持，改用地址嵌套地址的真实未支持形状；
   在同三个位置保留一层独占地址的 shape admission 正例，其余预算与支持域断言不变。
2. `closures_frontend` 的 c014_06 保留 `let f = () => { x = 2; ... }` 原源码。
   默认独占捕获已实现，但不可写闭包位置仍不能调用 FnMut；精确预期改为
   `SafeReject / E-CLOSURE-MUTABILITY`，不把源码改成 var 来隐藏拒绝。
3. `impl034` 的跨模块 CT 记录名义身份正例将 `Point` 与外部读取的字段 x 显式标为 pub，
   普通调用与 CT 物化的既有成功断言保留。复用同一工厂和消费者追加私有字段子例，
   普通/CT 两条路径都要求 Types 阶段 `E-FIELD-PRIVATE` 且无 executable。

root 随后追加两个同类协议失败：bootstrap 的 unsupported 源和 CLI/IDE 诊断对照的内联
unsupported 源仍用默认写捕获。将这两个 Unsupported 输入改为合法但未开放的嵌套地址；
不可写 FnMut 的原拒绝继续由 c014_06 精确覆盖，原 fixture 文本在收据保留。
failure-chain 保持语法、类型、Unsupported 三类和无 executable 断言；diagnostic-parity
保持八个输入、实际 CLI/IDE 进程和完整诊断一致性断言。

依据当前规范第 4、9、14 章及 042/043 已审核实施范围。写集仅为本提案、上述三个测试文件、
`examples/bootstrap/unsupported.zil`、`tests/bootstrap/failure-chain.py`、
`tests/incremental/diagnostic-parity.py` 和 `evidence/integration/draft08-test-migration.json`。
不修改工单、handoff、最终摘要或公共 schema。

限定验证仅运行 `zil-analyze --test budgets` 与
`zil-driver --test closures_frontend --test impl034`，以及新增授权的 failure-chain 和
diagnostic-parity 脚本，使用独立 target；脚本通过 ZIL_BIN 消费 root 已构建的当前生产 CLI。
不运行全门。
原统一门失败日志保持原件，迁移的实际命令、输出与哈希进入紧凑收据。
候选交由 root 非作者复核后集成。

## Rust 目标

本次只恢复既有测试对当前接受域和隐私的准确约束，不增加语言能力或性能证据。
完整安全、运行/内存、表达/维护、编译成本和至少一项突破均维持已有未证明结论。
