# 242 作者限定交付

实现提交 `ecd5a44bdb83c866bb6b1f5c12f8a8e9a2aa3ff1`；最终规范为 root 的 e3b8f374，摘要见 REPORT.json。只实施 Verify 与其限定测试，不代表非作者审核、合流或正式全量通过。

独立核重建 Enum-only typed path 的实际 local 激活、完整 prefix 和选中 tag；mutation 同时撤销旧观察 SSA 的授权，合流只保共同事实。Unpack 先核全部目的位置再原子转交，实际 fields 逆序初始化；保留旧责任、条件 Drop、scope、defer、借用与来源检查。普通 Value 返回每叶按既有 Incoming 处理；真实根/tag 路由不能授予 create。只有原 Managed create 合同保留新鲜性权限。

最终同一源码的 Verify 单 crate 291/0 已包含新增 14 项；不是 291 加 14。严格 Clippy 通过。source harness 的 5/0 分为四项已提交的源测试，以及一个树外五模块 244 默认预算探针。前者包含六个接受源、两个普通函数返回 create 拒绝源，每源都直接 raw/Wire Verify，完全不调用 Analyze。随后单独运行五模块预算测试 1/0、5 filtered；不能冒称一次六项全跑。

新增右嵌套 raw Verify 的精确 N 为 13709；原 244 五模块输入为 4848450；两者 N−1 ResourceLimit、fresh N 成功。旧 defer Verify N=3324 不变。原 244 默认 10m 超限失败保留；实际减少重复 Content 拷贝、合流和 summary 调度后成功，默认额度和收费单位未调低。126ms 等单次观察不是编译性能基准。

保留的重要原失败：534 曾错误允许普通恒等 relay 保 Created，此与既有 §8.1.1 冲突，经 root 审查纠正；原接受测试与日志保存。普通 identity 和非 identity factory 现在都有 raw/Wire、真实源反例。右嵌套源此前拒绝的原因是前端在新绑定后清空但仍 Live 的 private shell；修复由 Types 作者 b77 完成，没有放宽核顺序。最早源候选遗漏字节/Store Desc Hold，以及旧两项 shape 预期迁移、编译/lint 失败也分列保存。

REPORT.json 给出命令、次数、工具链、实际边界和发现；SOURCE-SHA256.json 绑定实现文件。源码/TypedIR/Wire、历史失败和树外 harness 位于 source-artifacts.tar.gz，并有逐文件 SHA。logs/ 保留所有开发轮次，不能将其通过数相加。242 没有执行 native/allocator 或正式全量；243/244 的机器及独审证据另行记账。

Rust 四目标仍有明确限制：有限安全边界和普通 Sum 组合表达有实证；此处的 Arc 只共享编译器元数据，没有新增语言运行时 RC。没有同语义 Rust 运行/RSS、维护成本或编译速度对照，也没有产品级四目标合取或净突破结论。产品门和默认限额保持。
