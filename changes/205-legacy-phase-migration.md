# 205 原始负例的精确诊断阶段迁移

root 正式 `workspace01` 在 clean `0317a093ade0118741d334c45438ade07e9d7228` 实际 2544/6/0。此处只处理主树205工单具名的四处旧断言，未更改任何 ZIL 输入、正例、机器预期或生产代码。原失败、原测试与实际输入单独保留；`baseline01` 的静态输入重构错误明确未运行编译器，`baseline02` 直接复制真实失败源并校验 SHA。

| 原测试位置 | 保持的输入 | 新精确边界 |
|---|---|---|
| 197 unused borrow definition | 267 B，SHA abf1ab9225dd61a989c14f56f119931d11cf8c6bc156f63fd202ce1be386dcf0 | Expand SafeReject / E-GENERIC-ARGUMENT / State alias outlives a moved or replaced field；Verify0 |
| 197 actual held Copy field window | 558 B，SHA 110bca62753be335abfd87cac7f2f488efa5aa8e4b126ab64d2fa4931afb945d | Expand，同一精确诊断；Verify0 |
| 201 same-element-two-exclusive-windows | 788 B，SHA 206789f4501312494b1fa60a0e9a5fe1d641b201ae740dd7c7e45b0e1b9aeed5 | Types SafeReject / E-STATE-ALIAS；该组前两个负例仍要求 Verify / E-BORROW-CONFLICT |
| 184 lent payload + whole-parent move | bad state.zil 288 B，SHA ce0913a8bc61de9bb8544584894b9a2d6a8f1915522b8f0d209ff7081af8e7f2 | Expand，同一精确诊断；Verify0，另外两个模块不改 |

未用197定义在旧实现被省略后只检查main，原Success不能作为坏正文已核验证据。新具名定义检查提前拒绝同一源，不等同于独立Verify执行。其它三处仍然是原安全反例，合法last-use、sibling grow和callee先准备控制的原生预期不变。有限运行将另保实际源码身份、命令、全部输入和观察；本说明本身不宣称通过、独审或整仓接受。完整1.4 NotSatisfied、B0 Doing、产品和原生Windows NotRun不变。

实际限定回归在 clean `94fba307fa447b5e29a6113d2cd653c07c2f838a` 通过：197混合State14、201既有独审测试13、184存储4，共31/0/0；既有正例 O0/O2 共52次原生执行。四个原输入（含184全部三模块）与首轮真实失败源逐字相同，均在指定早期阶段 SafeReject、Verify0、无执行产物；201另两个负例仍实际Verify1/E-BORROW-CONFLICT。此为作者限定复测，不替代非作者最终审核及root整仓。Handoff实际仍Fail1138，原样记录。
