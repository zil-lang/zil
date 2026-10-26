# 016：局部标量共享地址与默认捕获

本切片在 1.3 捕获规则下开放默认只读标量捕获，保持 `move` 创建时快照。它不改变语言规则，不宣称完成投影、独占、资源或可逃逸闭包。

## 表示与边界

- 根为所在函数的 `LocalId`，遮蔽声明具有不同身份；共享地址为 `Address<false, scalar>`。捕获创建发出 `BorrowLocal`，闭包每次读取捕获名发出 `ReadAddress`，LLVM 环境保存真实栈指针。
- `StorageLive` / `StorageDead` 记录词法激活及退出，包括循环控制路径。机器栈槽先保留到函数返回，标量与 Unit ABI 不变；Unit 被取址时有独立字节槽。
- 候选 `ValueHold` 对每个 SSA 值提供排序去重的 `Local` / `Capture` 根。analyze 与 verify 分别从原始 CFG 重算来源；verify 不导入 analyze 求证代码。来源状态只存实际出现的关系：analyze 用树集合，verify 用排序向量，CFG 仅记录非空局部载体，不建立 CFG×来源×对象矩阵。
- 有界反向活跃性覆盖 SSA 与局部载体。共享 Hold 的真实最后使用前，拒绝相交 Store / StorageLive / StorageDead；闭包副本、条件与循环沿同一规则传播。
- 普通函数与闭包参数、返回保持标量，环境地址不能从普通调用或返回逃逸。源码显式 `&x` / `*x`、借用参数、FnMut、嵌套闭包、资源、聚合投影继续 Unsupported。

## 验收

真实源码验证默认借入及 move 快照、所有副本的最后使用、分支/循环/遮蔽、越域与冲突拒绝；原始 IR 验证伪造/遗漏 Hold、失效地址、非法来源、预算和入口边界；LLVM/native 对照独立模型。作者只跑限定检查，精确提交独审后由集成者运行统一门。

## 逻辑预算迁移

初始化传递由预汇总 Store 集合改成按指令顺序执行 Store/StorageLive/StorageDead；新增密集 ValueHold 输出。原精确工作预算测试随算法成本更新：100 常量样本基项 630→828，双参数空函数 100→96，8 空块链 198→182，目标文本计费保持三遍。每个新值都仍验证“恰好预算通过／少一单位 ResourceLimit”；未改 Limits 默认值或正式性能门。旧 Address 未实现例改为仍未实现的 mutable Address，原 Unsupported 断言不变。
