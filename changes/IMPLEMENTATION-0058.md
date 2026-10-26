# IMPLEMENTATION-0058 — 同步错误与清理作用域独立核验

任务：058。作者：`/root/error_verify`。共同表示基线：
`25d2a574cecda2fd8af32527f3a73f1d0c9c14e9`（schema/wire 8）。

## 实现与信任边界

- `Type::ErrorUnion { error, result }` 的两个payload由Verify自己的有限Copy
  类型遍历检查；不导入Types/Analyze求证，不依赖producer的`is_copy_value`。
  三条error指令检查精确payload/result类型，并参与普通SSA ID/支配/使用顺序检查。
- `cleanup.rs`依据真实CFG独立重建scope栈和`StorageLive`的owner；每个ID只有
  一个语法enter，允许循环在先exit后重用该enter。ID是稀疏标识，不按最大ID分配。
  根scope绑定已存在的参数storage，局部读写、borrow、give、drop及局部闭包调用
  需要活跃storage；每个scope退出前其storage必须全部`StorageDead`。
- 汇合和循环回边必须具有完全相同的scope栈和storage owner状态。Return必须
  退出全部scope。Error标记必须在同一控制边普通Return一个实际
  `MakeErrorUnion { is_error: true }`产生的SSA；不把marker当作tag证明，未知
  来源明确拒绝。Return标记允许任意tag，不能被理解为success证明。
- Break/Continue标记要求普通jump并保留外层scope；核验真实控制边的scope/责任
  一致性，不凭marker重建或声称证明源代码loop label选择。
- 责任仍由既有独立resource状态机根据真实消费/初始化顺序检查；error return
  不豁免漏清、重复消费、重复自动清理、逆序清理、未提交拥有SSA或环境责任。
  错误退出使用普通可返回tag值。Trap属于abort，不能满足Error出口契约。
- 纯CT只新增已验证scope enter/exit的白名单；ErrorUnion仍被CT的有限可序列化
  类型集合拒绝，没有借scope标记扩大CT的error/runtime能力。

## 精确范围与未完成项

任何含scope指令、error操作或嵌套ErrorUnion类型的函数都必须满足完整scope
合同。无error且无scope的历史raw IR保留原Storage/责任/Return检查；不宣称这些
未标记夹具具有细分词法scope证明。源码前端统一scope化由057负责。

**当前scope合同归属storage，不归属SSA结果。** 内层块产生的拥有SSA可在exit
后转交外层storage并正确消费；该正例不能误拒。拥有SSA的独立检查仍是全函数
守恒。其未提交给调用/外层storage而到达Error/Return时必须被拒绝。测试同时
覆盖这两个近邻，不能把接受块结果转交写成“scope出口所有SSA债务为零”。

有限Copy error payload不包括Managed、Desc、地址或拥有闭包；未增加部分构造
资源error payload、异步取消、panic展开或完整语言终态。错误scope checker不
独立证明跨编译阶段的源语义保真；源码/本机运行及全量必要门禁由057/056和root
集成核验，不能以本单raw/wire夹具代替。

## 验证记录

限定运行：`cargo test --offline --locked -p zil-verify`，独立target目录
`/workspace/scratch/88febaa630a7/target-058`。新增边界测试覆盖tag/payload伪造、
SSA越界/支配、scope身份/栈/storage混流、循环重启、参数根scope、资源/闭包
error清理、完整构造伪造、返回责任重耗、wire重入及CT能力边界。

施工中的首次联编修正了wire测试调用缺少Limits参数；两次限定测试只暴露测试
断言的诊断文字不匹配，分别95/96、100/101通过，未修改生产准入规则来迎合测试。
生产及限定受测提交：`118b06dccc024cf78c616013d80ca1a8c383077e`。
在该干净提交上实跑101项unit（新增28项）和5项compile-fail文档测试，全部通过；
限定`cargo clippy --offline --locked -p zil-verify --all-targets -- -D warnings`、
`cargo fmt -p zil-verify --check`及`git diff --check`全部通过。命令、日志和SHA256
保留在`evidence/impl/058/acceptance.json`；随后证据提交不改生产代码。

Rust四目标：本单提供有限同步error/storage scope的raw独立安全证据；完整语言
安全仍未完成。运行/内存、源表达维护、完整编译成本均未测，至少一项净突破未证。
源至本机路径由057/056集成补证，不把本单28项边界测试扩写为产品完成。

状态：施工、等待非作者对精确交付审核；本文件不把作者限定测试作为独立审核或
全量验收。
