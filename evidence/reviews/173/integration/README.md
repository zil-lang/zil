# 173 组合接线独立审核

**结论：通过本次限定审核。** 审核绑定集成候选 `6a623edef1e8d928b5b11091a50f9319fbe6fd59`，实测编译器来自 `02cce89206e74cb5aa0050faf4885973288b9383`，SHA-256 `623dcf1d58f4dc5c0cb02dff9c912e2a5a866d5d0c7dc9188878114e8d6e3c73`。两者差异仅为五个测试模块和两份变更记录；已逐路径核对，生产与消费者文件相同。047、147、163 的原共享头源码字符串均保留。

实现作者为 `/root`，审核者为 `/root/dynamic_store`。本审核只覆盖根作者的 173 接线、owned-error 普通函数上下文补项、消费者/测试探针和预期迁移；**不审批审核者自己编写的 172**。172 由另一位代理独审，公共 IR/Wire 的既有非作者审核也不在此重复。

静态核查未发现阻断问题。`context_signature` 仍在 clone 前收取完整类型和合同遍历工作；新形状分类只恢复既有普通错误 ABI。合并保留 shared head、Store 和拥有数组字段各自的捕获禁区。

独立推导析构序后，在实际 CLI 上运行根消费者 44 次：O0/O2 分别构建，14 次普通运行、30 次实际 allocator 观测。运行时长度 0/1/3/4、初次 Store 分配失败、替换 RHS 每准备点失败、取走整个数组字段后错误退出，均符合 `STATIC.md` 的先验推导。没有从空stderr补造零分配，零分配由测试构造器注册的同一真实报告器记录。

另外写了不含 Store 的异形源码：四模块 `P!i32`/`void!P` 普通参数/返回与保留 FnMut，O0/O2 及任一 payload reserve 失败均输出 ABC/exit0；双拥有载荷 `E!Q` 逐tag清理输出 DEC/exit0。共12次，其中8次有实际 allocator 账目，全部 `allocated=frees`、`live=0`。相同两个程序在根上下文修复前的编译器均真实被 Types 拒绝。

8个最近反例全部在语义阶段拒绝：错误联合的借入/move/数组捕获在 Types 拒绝；借入资源和嵌套拥有错误 payload 在 Types 拒绝；非 Copy 闭包参数/结果在 Analyze 明确 Unsupported；重复消费错误值在 Analyze 返回 E-UNINITIALIZED。没有将解析错误、工具失败或任意拒绝当成目标反例。`CHECKS.json` 逐项核对确切 Phase/Code/消息。

`REVIEW.json` 为机器结论，`SOURCE.json` 与 `IDENTITY-MIGRATION.json` 绑定输入身份，`composed/`、`probe-results/` 保存命令与原始观察，`probe-inputs/` 和 `probes.py` 可重放新增探针。原生执行文件、构建缓存和重复大阶段转储不随本审计片收录。全仓正式验收与产品门另由集成方记录，不在本审核结论中推定通过。
