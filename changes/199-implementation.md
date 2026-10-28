# 199：直接 State 回调字段调用保留实际位置

本片候选为 `05f8e31cb1647385b2d96d6114b04630396a84e9`，生产变更为 `22f74e02bb3cb0c4c840a04ba9951f91c38374fa`，等待非作者验收。设计摘要 `09ef7fcdf2e71c2de38456d45d911aadc02dec3f355c4edaf8b51f2feb689c51`，规范集合摘要 `92ede13ac8b9042f08a7b0d737abe6502f79eb6a62ffed8d92d77416585ed1a5`，沿用 schema/Wire 25。本片不改变公共表示、Analyze、Verify 或默认额度。

## 问题与实现

197 的原始 `current.callback(&mut current.value)` 循环被当成普通字段取值：先 TakePackageField，再借临时 closure 调用，导致回边第二次 take 缺少完整字段。此前显式分别借 `callback`、`value` 的程序可运行。199 对直接 `Name.member(...)` 且真实本地 Package 字段为具体 Closure 的调用，先定位实际父 local/字段，再按接口模式建立调用位置；参数仍按源码顺序求值一次。

- Fn 发共享 BorrowPackageField；FnMut 发独占 BorrowPackageField，均经已有 CallClosureBorrowed 调用实际环境。FnMut 要求父 Package 可写，不因位置不可写退回消费模式。
- FnOnce 发 TakePackageField，经既有 callee 暂存与 CallClosureOnce 转交责任；参数准备失败仍结算已取出的环境。
- owning State specialization 原先清空 symbolic callable parameters。实例化现在将唯一已绑定的 `(ClosureId, CallableKind)` 作为 Builder 私有状态保留，使实际 Fn 环境在只声明 FnOnce 的入口按消费适配，在声明 FnMut 的入口按独占适配调用。这不是公共的免检合同；实际 IR 仍由安全核独立核验。
- 一般 `let callback = state.callback` 仍走原 TakePackageField；take/refill、字段替换和整个 State 转交规则不变。共享字段地址构造提为复用 helper，直接调用与显式字段借址使用相同位置检查。
- 新增作用域查找、字段比较、捕获类型快照与借址准备在分配/复制前计费；查询直接读取已有声明，不为查找克隆整个 Package 类型。

197 原 direct 字符串保留 573 字节，SHA-256 为 `f44c031e574bfeb1da67ad6cb2a26eb5c84faf78c8a1acbbf520c246c9769b4e`。与 f360417b 的原夹具及 199 的 ORIGINAL 常量逐字相同。旧测试只将该入口的拒绝断言迁为 O0/O2 正例，显式 sibling 借址对照继续执行；历史失败收据未追改。

## 作者限定实测

`impl199_callback_place` 与 `impl199_callback_place_raw` 最终 15 个测试组通过。实际 source04 有 77 次源码编译：64 次 O0/O2 原生执行成功，13 个正确拒绝分别为 Expand 4、Types 1、Analyze 1、Verify 7。原197测试的单项迁移另跑 1 个测试组及4次原生执行，与新夹具的重复控制不合算为新能力。

源码包含普通 State<F> 与混合 State<T,F> 的 Fn/FnMut 0/1/多次调用、break/continue 和错误返回、真实 sibling 修改与完整 State 转交；真实 FnOnce 和 Fn→FnOnce 的环境释放早于后续语句（AXT），零调用仍清理 TA。普通 take/refill、参数副作用一次、参数 `?` 失败、同字段活跃窗口、callee 已建立后参数中的重入/父移动、父替换与 lending 最后使用均保留相邻正反例。

raw/Wire 共16组输入，各路线独立核验：7个源码控制、9个攻击。攻击覆盖错误字段、共同伪造共享权限及 SSA 类型、借入调用伪称 FnOnce、删除 State cleanup、旧字段地址在替换后继续调用、类型相同但已经移动的实际父 local、FnOnce 以普通读替代 take、把真正 FnOnce 环境改为共享借入调用，以及将第二次 callee 借址移到前次 lending 最后读取之前。最后一组保持原始源码正控制和实际 SSA 图，Verify 重建后拒绝；未调用 Analyze/Types 求证函数代替 Verify。

| 独立阶段 | 最小通过工作量 N | N−1 | 新建状态重试 N |
|---|---:|---|---|
| Types | 3244 | ResourceLimit | Pass，完整模块相同 |
| Analyze | 14450 | ResourceLimit | Pass，完整候选相同 |
| Verify | 22203 | ResourceLimit | Pass |

四模块 `examples/mainline/callback_place199` 使用真实 direct 字段调用，State 与 callback 各拥有一个 ByteBuffer。Linux O0/O2 消费者已实际执行86次（普通14、独立 allocator probe 72），其中52个真实分配故障逐点注入，预期 stdout/exit、分配/释放、live=0 全部匹配。消费者在21043cf6执行，生产四文件、消费者脚本及四个输入与05f8e31c字节相同；没有将测试/文档提交差异当作生产变化。Win64/Wine按其独立收据报告；原生Windows没有执行。

最初三轮中的夹具阶段/代码预期错误均保留：同字段/父移动实际由 Verify 拒绝；死父 local 更早由初始化检查拒绝；用于观察 FnOnce 清理的 unsafe 泛型语句触发现有 Expand Unsupported，改为调用普通 emit 函数后实际 AXT 通过。这些失败不记入最终通过数，也未通过修改安全核制造接受。

## 当前边界及四目标

本片只扩大明确的真实本地直接字段调用入口。一般临时 carrier、Group/Sequence 位置、多个环境角色的拥有 State 未被批量放开。已另留实际探针：泛型体内新建同工厂 State 并直接调用、泛型体内嵌套 move closure、显式借参同时返回 lending 结果等现有前门 Unsupported 不算安全反例。外借描述在移动到 owned callback 后仍使用的探针在实际头转移检查处被拒，亦没有冒充“当前最后一次 call”证明。已支持的 Copy 参数加 lending 结果则有真实最后使用正反例和 raw/Wire 重排反例。

| 目标 | 本片实证及限制 |
|---|---|
| Rust同类安全 | 字段/父 local/模式/责任与最后使用的实际 source、raw/Wire 证明增加；不代表完整语言安全成立 |
| 运行与内存同级 | 有实际 O0/O2 行为、布局路径和逐分配失败清理；没有同语义 Rust 时间/RSS 对比 |
| 表达与维护更简单 | 原 direct callback 语法可与 sibling 数据、循环和错误清理组合；没有维护成本比较 |
| 编译成本更低 | 工作量按原额度有界、N/N−1可复核；没有编译性能采样或优势结论 |
| 至少一项突破 | 197 已保留缺口得到实际修复；不将工程能力增量宣称为产品级净突破 |

完整语言 NotSatisfied、产品 NotRun、B0 Doing；本片完成不能替代用户1.4设计的全部终态条件。作者证据只作候选材料，正式裁决由非作者审核及集成记录给出。
