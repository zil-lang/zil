# 201 提案：Store 的元素拥有具体回调

状态：源码审计提案；尚未派工、修改规范或扩大生产准入。依赖 200 的正式冻结，不把当前提案计为能力完成。本文按 200 候选及现行 1.4 / Ante-R1 规范识别下一条纵向缺口。

## 要关闭的实际输入

`crates/zil-driver/tests/impl200_fixtures/02_store_of_callback_state.zil` 是已保存的原始输入：一个自含回调和 ByteBuffer 构成 `State<i32,F>`，普通泛型函数创建 `Store<State<i32,F>>`，reserve 后转交完整 State。当前明确在 Expand 的 StoreElement 资格门拒绝、Verify 未执行；这一结果不是借用安全证明。

目标是让完整、自含的同质回调 State 使用现有 Store 的 reserve、push、pop、失败返回与前缀清理。优先复用 200 的同 F 一或两直接字段、197 的完整 T/F 名义身份及已有 Store 窗口。不同 F/G、动态擦除、递归拥有类型和外部保留回调不由此自动开放。

必须先使原输入原字节进入 Types、Analyze、Verify 并实际运行，再用多个独立元素检验责任。不得只允许 Store 的类型声明，却漏掉元素回调清理或在窗口失效后继续调用。

## 源码所示的连续阻塞

| 层 | 当前入口及具体缺口 | 必须保持的责任 |
|---|---|---|
| Expand | `generic_store.rs` 把 Package 的非 manages 字段送入普通 Copy 存储检查；F 不具备这项资格 | callable 占位必须来自已核泛型角色及有限 State 形状，不能把 F 伪装成普通 Copy 或允许用户空 impl 制造 StoreElement |
| Types | `dynamic_store::supported_element` 只接受 Copy 和有限直接 Managed 字段 | 对实际 ClosureId 独立核自含环境、字段数、类型身份及布局配额；不能信任前端分类布尔值 |
| 特化 | `callable_fields::specialize` 已递归处理 OwnedStore/Managed/Desc/错误联合 | 仍须实际验证完整元素名义键和闭包身份进入 sealed Store 定义；同布局不同类型不能合并 |
| Analyze | Store 操作及元素资源检查分布于 `dynamic_store.rs`、`budget.rs`、`resources.rs` | push 成功消费、push 失败保留错误载荷、pop 成功转交与空前缀各自重建；所有 callback 字段均按实际位置负责 |
| Verify | `dynamic_store::{check_element,layout}` 排除 Closure；独立布局没有具体闭包环境参数 | 验证 Closure 定义和真实布局，先计费后展开；不能使用 LLVM 的字节结果作安全证明，也不能跳过类型 DAG/循环检查 |
| LLVM | `emit/dynamic_store.rs` 用空 closure 表求元素对齐，前缀 drop 循环只处理 Managed 字段 | 必须使用实际环境布局，并逐字段调用已有闭包清理；全元素转交、原位置撤销与声明序清理保持 |

因此，只打开 Expand 或 Types 会产生不完整实现；特别是原 LLVM 前缀清理会遗漏新增回调字段。此问题必须与实际机器路径同批解决。

## 表示与边界需要先确认

1. 现有 `Type::Package`、`Type::Closure`、`OwnedStore`、push/pop 指令可能足以表达完整值转交，但这只是待验证假设。发现缺必要事实时先修改公共接口，不伪造无效果合同。
2. Store 的元素地址仍是借入窗口。需要分别核验 callback 的原位置借入、窗口存活期间的 reserve/push/pop/give/drop，以及 FnOnce 从借入元素窃取责任的拒绝。若现有源面或独立来源表示不能表达一项操作，准确列出阻塞，不能把资格期拒绝记为窗口安全通过。
3. 可先借现有单 State 参数入口，在函数内部使用 Store。返回拥有 Store、两个 State 参数、State 加 replacement F 等入口是否需要额外泛型表示，应由实际输入确定；不能顺带免除 200 明确保留的边界。
4. 新元素允许回调后，必须检查闭包捕获 Package/Managed Store 的递归路径。内联布局环、经 Store 隐藏的环境依赖及过深类型都不能依赖无限递归遍历；既有有限类型和预算保护继续有效。
5. Store 自己的运行时头、初始化前缀、失败保持和保守结构失效协议不变。相同分配地址、字段名或 ClosureId 均不能恢复失效窗口或合并两个元素的责任。

## 必需的实际验收

- 原 02 原字节正向贯通；普通非回调 Store 原正反例和精确旧预算保持。
- 一或两 callback 字段、普通 Copy T、同一工厂的不同运行时状态；多元素的独立值、跨函数完整转交、逐元素调用、pop 后继续调用和清理。
- reserve 失败保持旧前缀；push 满容量返回原完整元素；pop 空/非空；每个实际分配失败点和最后分配后控制；声明序字段清理与已核元素清理顺序。
- 共享/独占窗口的真实正反例；同 Store 别名、普通 helper 参数、callee 准备和活动 lending 结果的父失效；FnOnce 消费只能来自合法拥有位置。
- raw/Wire 攻击覆盖实际元素类型、ClosureId、缺字段、重复值、漏清理、伪造自含捕获、Store 定义名/布局以及伪造返回载荷；为新前门拒绝单列 Unsupported，不冒充责任检查。
- 非作者重建源码与机器行为；Linux O0/O2 和 Win64/Wine 分列，原生 Windows 未运行仍写 NotRun。默认配额、产品阈值和编译速度独立支线不改变。

200 的进展不因本提案扩大验收范围；完整语言和产品终态仍由现有规范与正式门决定。
