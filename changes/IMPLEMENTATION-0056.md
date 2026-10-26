# 056 同步错误联合、显式清理作用域与机器路径

基线为 draft10。规范已有 `E!T` 和表达式后缀 `?`，但原实现只支持用户 Outcome 的普通 Return，无法表示传播出口；本片落实 §7/10/14 的同步部分。同步错误不变成 checked trap，不增加异常运行时、隐式 Box/RC 或第二套责任系统。

## 公共表示与源面补全

- `Type::ErrorUnion { error, result }` 保留两个类型及区别于用户 enum 的身份。当前实现只接受有限 Copy 载荷；拥有结果、地址、取消与一般 unwind 仍独立列为未完成。
- `MakeErrorUnion { is_error, value }`、`ErrorTag { value }`、`ReadErrorUnion { value, is_error }` 分别构造、观察 tag 和检查 tag 后提取。true 为错误，false 为成功；错误投影具有定义的 trap，不生成未定义值。
- 源码使用已有 `return expr`。有明确 `E!T` 期望时，完全相同联合可直接传递；普通值与恰好一侧类型精确相同时注入该侧。若两侧同时匹配则诊断歧义，不能以检查失败回溯选择。`?` 只接受错误联合，在当前返回 `E!U` 且错误类型精确相同的函数内传播；无隐式错误类型转换。
- 规范尚未固定以上构造细节；本片由集成方以最近反例（`i32!i32` 普通返回无法唯一判支、非错误 `x?`、错误类型不一致）补全，保持现有后缀问号，不启用前缀/捕获候选。
- native `main -> E!i32` 成功返回其 i32 状态；`E!void` 成功为0，错误为1且不隐式打印。源函数返回之前已经完成合法清理，wrapper只检查tag。

## 清理合同

`ScopeEnter { scope }` 与 `ScopeExit { scope, kind }` 表达函数根及实际词法激活；kind 为 Fallthrough/Return/Error/Break/Continue。StorageLive 的位置属于当前激活，退出前必须先执行其真实 Drop 与 StorageDead。参数属于函数根；完整表达式资源临时登记实际存储。不同 CFG 前驱的 scope 栈必须一致；循环重入允许同一静态 scope 重新激活，但不能重叠激活。

`?` 错误分支在源位置取得错误值，再用清理列表快照退出当前所有激活；不能破坏成功分支的待清理列表。构造字段逐个求值，全部成功才发布 MakeManaged；失败前未完成的资源不执行完整 teardown，先前已完成资源仍按逆初始化顺序释放。当前字段均为Copy，此合同不冒充含拥有字段的部分聚合构造。

所有本轮源前端函数生成显式scope。含scope指令或错误联合的原始IR必须完整核验；无错误且无scope的既有手工原始IR继续接受原有独立责任/StorageDead/Return检查，不把它宣称为细分词法scope证明。原始wire版本升至8，旧字节版本拒绝。

纯CT函数也经过同一scope核验。VM执行scope标记不改变值，但真实指令访问照常扣逻辑step；不能为了保留旧夹具的数字把新工作改为不计费。单个`#(zero())`由原4步增为12步（查询入口与callee各6步），累积配额测试相应核对一个查询12步通过、两个查询12步拒绝、两个查询24步通过。生产默认配额与产品门没有提高。

## 验收

独立Verify检查每个出口和原始wire反例；后端只消费同一Verified CFG，使用内联tag与载荷及现有真实live flag。真实多模块程序覆盖直接错误、嵌套传播、创建前后失败、返回临时、未调用拥有闭包与Consume后的错误清理。ZIL/Rust O0/O2按stdout、退出码和stderr比较；错误联合仅有类型名或手写IR不记源码贯通。
