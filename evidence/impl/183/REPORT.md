# 183 直接拥有资源成功载荷：作者交付

实现提交：`19f58c16ed800b0cee5a8afcc11908925920cd63`。本片限定作者验收 Pass，独立审核待另一代理执行。schema 22 / ZIL-LANG-1.4，规范摘要见 `source-receipt.json`。

## 实际能力

显式 `E!R result(create/give)` 与 `give x:E!R` 的成功侧现在是 Managed，外层保持普通非 Copy Value；默认资源描述类别不变，`give union` 仍拒绝。ByteBuffer、有限名义用户资源和具体 Store<Package> 均有真实执行。普通函数、跨模块适配、`?`、直接丢弃、保存在局部、晚实参失败、部分聚合准备失败及循环退出保留唯一责任。

前端按模式选择成功载荷。Analyze 和独立 Verify 分别追踪成功资源的创建/输入来源；包装动作本身不制造 fresh，错误注入不增加成功侧来源。Verify 仍独立查名义资源、真实 tag、一次 Take、漏清与描述来源，不调用 Types/Analyze 的安全判定。机器层继续使用内联 `{tag,error,success}` 表示，先清责任标志，再仅清活动载荷；直接资源沿既有 teardown 路径执行，没有为联合新增隐式堆盒或引用计数。

## 验收范围

最终干净提交上的限定测试为 **92 / 0 / 0**：既有资源方法25、Package错误联合13、资源头17、Store16、目标平台3，本片源测试13和raw测试5。新片有32次O0/O2原生执行、18种raw变异的36条raw/Wire核验路线，以及17个精确源码拒绝（含既有原生main边界）。原159 ByteBuffer探针输入逐字相同并通过。

四模块消费者同时贯通 `Error!Mark`、`Error!ByteBuffer`、`Error!Store<Item>`。Linux及Wine下的实际Windows PE各62次，共124次；每个平台包含14次普通程序、48次探针，其中36次真实分配或增长失败。全部分配与释放平衡、live为0；Win64探针bad_operations为0。程序校验实际存储数据，Drop轨迹区分工厂错误、调用方错误、成功联合丢弃和用户资源尚未创建的错误。Wine不等于原生Windows认证。

## 哈希与复用边界

机器运行的实际提交是 `3d146206676fce98c53ca183aaeb4d7164fe7fbd`。之后仅修改raw测试中一个Box替换写法，以通过Clippy；编译器、消费者、runner及规范没有改动。CLI构建于 `0d19b096b574156b3d54fe3589f1d03e99eb93c2`，到最终提交只改测试与消费者。`compiler-and-runtime-binding.json` 列出全部差异，并逐文件比较编译器源码、Cargo和规范输入完全相同；不会把旧运行日志的提交号改成新号。最终92项、fmt和新测试Clippy是在19f上重新执行；生产Clippy记录保留原实际提交。

## 原失败均保留

开发过程中的测试API误用、Analyze形状入口遗漏、测试访问私有字段、错误Cargo包名、宿主rlib异常、示例unsafe表达式/重复外部声明，以及测试replace_box告警均保留实际日志。宿主rlib问题用同源新target消除，精确缓存原因没有独立定位；没有因此改语言语义或预算。所有最终所述Pass对应实际完成的运行，未把早期失败覆盖或累计进最终通过次数。

## 仍未开放

泛型 `E!Store<T>` 的符号成功模式另属186，不能由具体Store实例外推。资源错误侧、任意拥有嵌套Sum、拥有联合捕获、整联合替换、直接资源数组类别解释及lending逃逸保持原边界。产品、完整语言、异步/GUI以及Rust四目标比较均未由本片完成；完整语言NotSatisfied、产品NotRun、B0 Doing。

本目录只含源码输入、命令、诊断、观测、哈希与明确标记的阶段摘录。执行文件、对象、完整LLVM/阶段dump及缓存留在运行目录，不进入源码或紧凑证据交付。相邻旧测试保留完整命令/计数，不重复收录其全部旧夹具产物。
