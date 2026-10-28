# 200 提案：同 F 的两个自含回调字段组成完整 State

状态：**只读源码审计和三份原始 CLI 预检完成；没有修改生产。** 200 的新有限规范、工单、写集及公共表示由 root 登记后才实施。审计使用主树 `40bb18edc408d38d5b1b55a2acdbcdf5266b94ec` 的 1.4 / Ante-R1 规范，实际编译器固定为 `f360417bf3de9c5a87164f2ffd87a5f414be5710`；不能把主树后续规范提交倒填为本次编译器身份。

## 选择与可关闭的具体缺口

建议 200 先允许 **一个完整 `State<T,F>` 保存两个直接、具体、自含、同 F 的回调字段，并由一个有限拥有构造入口接收两个 F 值完成装配**。普通 Copy T 可省略；单字段 State 保持。一个组件可以持久保存由同一工厂产生、具有不同运行时状态的两个处理器，经普通函数返回和移动后分别调用、替换、消费或清理。

这直接连接三项现有能力：196 的两个同 F 实参、197 的完整普通 T 身份、199 正在实施的真实回调字段调用位置。它不需要先开放完整 trait 求解、卫生 UI DSL、动态回调容器或一般 F/G State。

200 的核心新增入口限两个直接 F 参数构造一个完整 State。已有单 State 参数的普通操作继续适用。首片不同时承诺 `State + replacement F` 泛型入口、两个 State 参数、不同 F/G 字段、嵌套 callback 字段或 callback Store 元素。具体 State 上原有已支持字段操作仍保持；不能以关闭这些扩展来改变既有行为。

构造入口按实际签名、字段及责任转交识别，不能特判函数名 `pack`。本片不凭空赋予两个参数 apart，也不把相同 ClosureId 解释成同一个环境值。

## 当前三份实际输入

源文件先提交为 `bbdb967d38e574a11d5a96518ed2213039dc6bd3`，随后各执行一次实际 `zil check --emit-dir ... --json`，共三次，无 native 执行。CLI SHA-256 为 `e83a70deeaa60a3616b3889af70fe80faab4f6f6984be7a84edeac0623cfdbd1`，与 196 的 `cli-build02` 及双平台消费者收据一致。

| 原始输入 | 当前实际结果 | 已到达的门 |
|---|---|---|
| `00_single_callback_state.zil` | Success，Verify 1 次 | 一个 `State<T,F>` 保存自含拥有闭包、普通数据与 ByteBuffer，跨普通 helper 返回和消费 |
| `01_same_f_two_callback_state.zil` | Expand Unsupported，Verify 0 | `owning State helpers require one environment role`，span `257..266`，指向第二个 F 参数 |
| `02_store_of_callback_state.zil` | Expand SafeReject，Verify 0 | `E-GENERIC-ARGUMENT`：`StoreElement fields require finite Copy or explicit direct manages resources`，span `58..59`，指向 callback 字段的 F |

三个输入的工厂都显式拥有一个 ByteBuffer，两个构造实参是独立调用同一工厂产生的环境，不靠不同闭包表达式冒充同 F。Store 输入保留一个直接 `manages bytes` 字段，因此其失败不是因为漏了当前 Package 的 Managed 资格。

第二项的 Unsupported 不是借用/重复消费安全证据；第三项虽分类为 SafeReject，仍只是 StoreElement 的资格前门，**不是已到达 Verify 的清理安全反例**。本次没有改写失败输入、没有运行第四个变体。原输入、命令、JSON 诊断、阶段文件和精确身份保存在 `evidence/preflight200/`。

200 的最小目标源形状就是第二份原输入：

```zil
package State<T,F> {
    manages bytes: ByteBuffer
    first: F
    second: F
    value: T
}

fn pack<T: Copy, F: Fn() -> i32>(first: F, second: F, value: T) -> State<T,F> {
    return State::<T,F> {
        bytes: bytes_new(), first: first, second: second, value: value
    }
}
```

完整原输入还包含实际拥有工厂、消费 State 的 helper 和 main；上面摘录不是另一份已经运行的探针。后续实施还必须用 FnMut、FnOnce、真实分配失败及原生运行补全验收。

## 源码根因：不能只删 Expand 的一个拒绝

以下定位以 `f360417b` 的生产代码为准，主树中尚在集成的旧源码不覆盖该候选事实。

| 位置 | 当前行为 | 200 所需最小改变 |
|---|---|---|
| `zil-expand/generic_callable.rs::template_with_state` | owning State 要求一个 callable 类型且只有一个环境参数；本次 01 实际先在这里拒绝 | 保持一个 callable 类型，只对已识别完整 State 构造入口允许两个直接、同模式 F 参数；其它 owning 多角色形状保持关闭 |
| `zil-expand/generic_state.rs::Engine::state_family` | 混合 T/F Package 要求一个直接 callable 类型槽和恰好一个直接 F 字段 | 类型槽仍一个；有限字段数从一改为一或二；原名义声明、完整普通 T、phantom T、模块身份及透明别名规则保持 |
| `zil-expand/generics.rs`、`generic_enum.rs`、`generic_check.rs` | owning 路径仍依赖唯一 callable 名；需区分一个 F 类型与两个参数出现位置 | 两个参数都约束同一 F；State 只贡献普通 T 推导，F 身份仍来自实际环境；定义期不从 Copy T 获得字段或调用权限 |
| `zil-types/callable_fields.rs::{template,signature}` | 以 `callable_parameters.len()==1` / `callable.len()==1` 判断 State 路线 | 按一个一致 callable 类型绑定与允许的构造签名识别；不能把普通双角色 helper 全部变成 State 构造 |
| `zil-types/callable_fields.rs::{environment,specialize}` | 已逐字段核同一实际 ClosureId，并遍历全部直接 F 字段；名义后缀仍只有一个 closure 身份 | 复用这一表示，确保两个输入均检查实际同 F；字段名、普通数据族及完整责任不能仅按布局相等合并 |
| `zil-types/helpers.rs::{validate_template,call_helper,instantiate_helper}` | owning 路径只接受一项；专门化时清除原 bound 并生成普通具体 Package 函数 | 为有限构造入口分别准备、暂存两个真实参数，再按同一绑定专门化；成功/失败各按真实责任清理；与 199 新的声明模式传递精确合流 |
| `zil-types/helper_plan.rs::{Shape::environment,Walk::shape}` | 已保存多个字段及其 ClosureId，但只在它们一致时折叠为一个具体环境类型身份 | 保留类型调度含义，不能把这个折叠当作运行时来源折叠；实际字段位置继续独立 |
| `zil-types/inference.rs::{state_term,bind_callable_argument}` | `ClosurePackage` 只带一个私有环境类型身份；shape 已能把多个同名 F 字段占位 | 复用一个 F 的结构，核对两个参数及 State 结果；不需要在本片改为 F/G 环境向量 |
| `zil-analyze/helpers.rs::check` | 无 public helper 合同的拥有 State 返回例外精确要求一个 by-value Closure 参数 | 独立识别最多两个同具体类型、自含、按值输入与完整两字段 State 返回边界；保留实际移动/构造/清理核验 |
| `zil-verify/helpers.rs::check_module` | 独立例外同样只有一个 by-value Closure 参数，且检查 self-contained 与实际 Package 类型 | 独立重建有限拥有构造边界，不能读取 Types 的分类布尔值授信；未知借入环境或缺失责任仍拒绝 |

表内文件路径均位于对应 `crates/<crate>/src/`。同 F 的两个真实环境必须各自走原来参数准备、Move、Package 装配、字段失效及 Drop 的检查，不能把列表去重后只检查/清理一个。

## 最小表示、规范与公共接口

建议先保持 schema 25 的形状：`CoreCallableParameter` 可以记录两个不同真实参数位置、同一个 `generic_index`；实例 `Bindings` 仍只使用一个 F 类型槽；`Type::Package` 已逐字段承载 `Type::Closure(id)`。相同 id 的两个字段在普通 IR 中仍是两个不同位置和真实值，不需要为它们制造两个不同类型。

拥有构造返回的是完整、自含的普通 Package；它不是 196 的不逃逸 callback helper。不能为了规避规则，给它伪造空的联合调用合同，也不能在 `CallEvent.parameter` 中编码 Package 字段号。现有单输入 State 构造已经有上述 Analyze/Verify 独立例外；200 只扩大这个明确、有限的拥有值边界，并继续核实际责任转交。构造前调用等附加行为不能因“构造入口”获得缺失的模式/效果合同。

基于当前表示，**预计不需新增 TypedIR/Wire 字段或改变机器布局**；这只是源码设计判断，尚未由实现和 raw/Wire 证明。若发现现有实际指令无法独立核验某项必要责任，须交由 root 固定公共表示，不能为了坚持“不升 schema”降低核验。任何 public 形状变更按原流程发布新 schema/指纹域。

需要 root 在 §18.1 的 197 单字段有限说明后登记 200 的精确增量，并与 §8.1、§9.3 的字段责任和调用模式一致。§8.2 StoreElement 的现有限定不变；196 直接 helper 的最多两个角色、联合轨迹与循环 FnOnce 规则不变。未调用模板仍检查；同 F 不允许混合声明模式；结构 Copy 资格不因 FnOnce 或“两个字段”而凭空增加/剥夺。

一般 `State<T,F,G>` 要另外解决一个真实 State 参数内多个 callable 泛型的绑定元数据。现有 Core 角色以真实参数位置唯一标识，不能给同一个参数 0 重复塞两个角色并宣称兼容；私有 `ClosurePackage` 单环境身份和 State family 键也须相应扩展。这是本片保持同 F 的具体原因。

## 最小派工与合流

建议实际写集从上表的 Expand/Types 文件、`zil-analyze/src/helpers.rs`、`zil-verify/src/helpers.rs` 开始；新的 source/raw/Wire 测试、四模块消费者和故障 runner 放在 `impl200*`、`examples/mainline/callback_state200/`、`tools/language/200/`，证据在 `evidence/impl/200/`。公共规范、schema、工单及集成身份由 root 单写。

前端作者负责 Expand/Types；199 作者完成其写集后负责 Analyze；root 负责规范/Verify；非作者另行审核。必须先合入 199 的精确提交再改相邻 `helpers.rs`：声明 FnOnce 的实际 Fn 环境仍按声明消费，不能因实际能力较强而改成借入。新代码不能整文件覆盖 199 的真实字段调用位置和声明模式修复。

LLVM 的普通 Package 清理已经面向逐字段类型；本片不预先申请后端行为扩张。若真实 native 暴露缺口，按确切根因登记写集，不用模型或 C 替代 ZIL 运行。

## 完成 200 必需的有限验收

1. 原 00、01 原字节复验：01 从前门拒绝进入普通 Verify；02 Store callback 仍在其旧资格边界拒绝，不把它改成正例。
2. 同一工厂的两个不同拥有环境，跨模块构造、完整移动/返回、零调用、只调用一字段及反复 FnMut 调用；独立计数和打印区分两字段。结合 199 的真实字段位置、已有 Copy T、显式切片及 break/continue。
3. 真正非 Copy 的 `pack(f,f)` 必须实际到责任/移动拒绝；同签名不同工厂不能冒充一个 F。另保留结构 Copy 闭包控制，避免把 196 原09的错误预期带回新片；不得靠改输入消除原规则差异。
4. FnOnce 某字段消费一次、另一字段未调用清理；重复消费、将缺字段 State 返回、错误声明模式及借入捕获保存拒绝。先准备后提交，后续准备失败只结算实际已持责任。
5. 同字段借入与 take/replace、父壳 move/drop 的近邻反例；独立 sibling 操作正例。两个字段即使 ClosureId 相同也不能复用同一地址或清理记录。
6. raw/Wire 攻击至少删第二字段、换名义数据族/实际 closure、把第二装配值改成第一份非 Copy SSA、漏第二清理、伪造自含捕获，以及在构造例外下偷渡借入环境/缺失 helper 合同。
7. O0/O2 普通产物分别执行；拥有缓冲的每次实际分配失败及最后一次分配之后的控制核唯一释放、清理顺序和零存活。Linux、Wine、Windows 原生分列，不把 C allocator probe 算成语言能力实现。
8. 构造、field scan、两次暂存、完整 T/F 键和实例计划先计费；实际 N/N−1/fresh 门闭合，原默认额度不增加。最终身份由正式规范冻结及非作者验收绑定。

## 完整 1.4 / Ante-R1 的剩余能力：按实际消费者排序

这张表根据当前源码入口重新分类；178–195 的旧报告中仍称未实现的 196/197 不能继续原样照抄。196/197 当前候选已有纵向实证；198 是离线交付能力，不填补语言语义；199 仍在施工，不能先登记完成。

| 顺序与消费者形状 | 已有基础 / 真实剩余 | 当前源码依据与下一步边界 |
|---|---|---|
| **200：一个组件的两个持久同 F handler** | 196 两个直接角色、197 单字段 State、199 真实调用位置；仍缺双拥有构造及双字段组合 | 上述三探针与具体函数；本片可独立关闭 |
| 同质组件列表，每个元素自带 handler | Store 的 reserve/push/pop、受限元素窗口及完整 Package 已有；callback 元素仍关闭 | `generic_store.rs` 的非 manages 字段转入 `generic_copy::concrete_storage`；Types `dynamic_store::supported_element`、Verify `dynamic_store::{check_element,layout}` 均排除 Closure。LLVM `emit/dynamic_store.rs` 的前缀 drop 只清 Managed 字段，**只放宽前端会漏闭包清理**。需独立纵切，不并入200 |
| 一组件保存不同签名/模式的 F/G、异构组件树 | 直接 F/G helper 已有；持久 F/G State、dyn、递归/异构拥有组合未闭合 | State family 的唯一 binder、`ClosurePackage` 单身份及 Core 同一实际参数内的字段绑定尚不足；`packages::supported` 仍是有限字段组合；完整 §8.1 Product/Sum/递归形状不能由有限成功类推 |
| 长期元素视图、文字/布局视图及通用 lending iterator | 共享/独占 Store 窗口、显式切片参数、有限环境 lending 已有；保存窗口及一般 life/HRTB 仍缺 | `lending::supported_result/check` 限非消费入口、共享结果、自含捕获及 Copy/ByteBuffer 环境来源；`generics` 明确拒绝 lifetime/const 语义求解。不能把已实现共享 lending 写成“完全没有 lending” |
| 普通可复用 Iterator/Widget trait | 有限静态名义方法、符号 bound、结构 Copy/StoreElement 已有；关联类型/常量、一般约束与封闭求解未完成 | `static_traits::collect_static_traits` 和 `generic_trait_bounds::declaration` 对 associated/default/generic/contract-bearing 方法有明确门；§14、§18.1及V10仍需逐项实现，不依赖完整trait才能先完成200 |
| 卫生 `ui!{...}`、反射派生与类型生成 | parser 已识别 `syntax use`、平衡 `SyntaxIsland`、`#emit` 外壳，格式化可保留输入；实际卫生扩展与 Type/Syntax CT 产物没有完成 | `zil-expand::expr` 对 SyntaxIsland 仍落入 Unsupported；CT materializer/CompileTimeValue 只有标量、聚合、枚举数据，不能当卫生 Syntax。需绑定作用域身份、调用点/定义点、源映射、冻结 impl 集及普通重检。此工作有独立价值，但不能修复组件拥有状态/清理缺口；排在结构组合之后，可独立准备 |
| 一般稳定图、独立删除与可组合失效协议 | 有限真实字段/区间、来源与资源路由已实现；完整 §3–§6、V04/V05/V12 尚不能据有限Store宣告完成 | `resources::definitions` 仍要求资源头有限 Copy 字段；Store 首片只有初始化前缀原语和保守结构 may 效果。稳定成员身份、显式间接载体及进一步协议需具体接口消费者；可由普通已核库承担的部分不先发明语言原语 |
| 一般 Drop/部分构造、消费循环及递归协议 | 现有有限 Package、拥有错误成功载荷、固定数组、Flow 与 break/continue 已有；任意组合、复杂递归协议和含 FnOnce 角色的循环仍有限制 | `packages::supported`、`resources::supported` 的有限形状及 `helper_flow::contract` 的 cyclic FnOnce 明确门。V07/V13须逐构造保留失败、回边及责任证据，不把普通 while 的通过当一般消费协议完成 |
| 平台长期 userdata/Active handler、挂起取消与并发 | 静态 C callback、同步 noescape、有限 Linux/Win64 ABI 已有；外部保留环境、no-future、Suspend/SABI、真实同步与Transfer/Share尚未闭合 | 现有 foreign profile 明确限同步、静态无环境 callback；普通 TypedCFG/ABI不能凭同步返回证明回调静止。§11–§12、V14–V16是高级ZUI及完整1.4义务，不强行设成手写同步组件200的前置 |
| 全部固定源面及CT类型词汇 | receiver、记录、管道、部分CT与普通类型泛型已有；不能据parser覆盖宣称源面全部可执行 | `generics` 的 const/where 门、`zil-expand::expr` 的一般 tuple argument expansion 门、`type_ref` 的有限基本类型及CT类型入口仍存在；规范示例的完整类型/生成能力需各自关闭。图形/字符串库若现有普通函数即可表达，先作为库消费者验证，不能仅因旧remaining出现就认定新增语言机制必需 |

§19 查询粒度、LLVM API、稳定编译身份与机器缓存仍属于用户分开的编译专项，不能用200的语言结果代替其门，也不把它们捆成200的实施前置。V17/V18及V19–V22仍要求最终同一产品身份、跨目标和完整成本证据；本审计未执行高成本全量或性能比较。

## Rust 四目标与状态

三份预检只确认当前可达入口和真实前门限制：不证明200安全、运行成本、维护成本或编译成本优势。所选切片的价值是让一个普通组件的两份拥有回调成为可复用状态，是否形成产品净突破仍须按原 `product-gates.json` 比较。完整语言 NotSatisfied、产品 NotRun 保持；本提案不下调门槛，也不把源码审计、Unsupported 或前端资格拒绝计作安全验收。
