# 0120 — 固定独立 ByteBuffer 字段的借用精度

## 问题与规范边界

恢复基线 `810346b9c8c7b99dac042c0edc3511904a9cd132` 的实际字段来源被粗化成整个 Local；因此 `bytes_view(p.b)` 活跃时，合法取走或替换独立拥有字段 `p.c` 被 Verify 误拒。本单重新实施，不把不可恢复的原113代码或测试记录作为当前证据。

依据既有 ZIL-ORIGIN、ZIL-PACKAGE、ZIL-BYTE-STORAGE：Package 没有自定义整体 Drop；每个 manages ByteBuffer 字段均有独立真实责任。设计反馈 DECISION-ZIL-BACKING-007 的 P1 进一步明确该正例。只细化已经检查的直接字段数字投影，不从变量拼写、两个 Ref 字段、布局地址不同或任意共享来源推导 payload 分离。

## 实施与安全论证

Analyze 用独立有序集合、Verify 用独立排序向量重建私有根 `(LocalId, field-index)`。每一处细化都核对实际局部类型中的该字段确为 `Managed<ByteBuffer>`；其余根保持原全局部/参数/capture 粗根。不同固定拥有字段可以分离；整个局部根和任一字段根相交。

借片/Desc 来源通过普通局部复制、分支合流、投影及函数返回来源代入传播。合流保留所有可能根，不能选择有利分支。Verify 的 shared Hold 失效检查、独占 loan 相交、byte helper apart 代入使用同一核验器内部的根关系，但不调用 Analyze/Types 求证。

这不是可永久复用的 backing 实例ID。对所借字段的 take/drop/replace、reserve/push，以及整体存储失效仍在活跃使用之前拒绝；因此同位置重新赋值不能使旧视图复活。Whole move 与稳定 payload 运输尚未证明，继续保守拒绝。Unknown 自定义字段 Drop 可能影响兄弟，仍按整个载体检查。

公共 schema/wire17 与 `ValueHold` 不变：Analyze 导出粗 `HoldRoot::Local` 并去重；Verify 自己的精确事实先作同样的保守投影，再逐值比对不可信候选。篡改候选不能制造字段分离。

## 接受性、预算与未改变项

接受性只放宽上面的独立字段正例；同字段、可能重合来源、祖先、整体移动/销毁、无证别名和未知 Drop 仍拒绝。独立字段 root 数可以多于粗 root 数，实际集合/向量工作和导出投影均记入现有确定逻辑预算，因此临界额度可能改变；默认额度不变，不宣称旧 N 边界等价。

不改变 ABI/机器布局、默认内存机制、公共 schema、规范或产品门。不加入 generation、RC、锁、装箱，也不改变 source 计算/析构顺序。旧正确性回归继续运行；不作速度或 Rust 产品级比较结论。

## 验证

真实源正例覆盖兄弟字段 take/refill/replace/reserve/push、两个独占窗口、helper/apart、拥有 FnMut/FnOnce 和多模块；最近反例覆盖同字段、分支合流同/异来源、祖先、整体移动、未知资源清理。TypedCFG 与公共 Hold 投影留证，独立 raw/wire 篡改候选与指令必须被拒绝。原生 O0/O2 输出和失败/剩余责任清理分别核对；原失败保留。仅做本单及受影响回归，不扩建 Roam。

## 旧契约迁移

受影响旧Verify原生IR回归曾要求实际两次ByteBufferNew装配的兄弟字段也在活跃期拒绝。首次重跑该期待失败已保留（final001）。主树120于root提交033247c8明确授权迁移为同字段/兄弟字段 × 活跃/末次使用后四格；同字段活跃拒绝及重复Managed SSA伪装不同字段反例保持。该迁移实现本提案的已声明接受性，不是删除失败或降低产品门。
