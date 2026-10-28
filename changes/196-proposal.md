# 196 预提案：同类型双环境与 F/G 多回调角色

状态：**只读预检完成，尚未修改生产或公共 schema。** 正式工单、规范文字与公共表示变更由 root 决定。本提案不合并 194 的「单 F + Copy T」路径。

## 已实际确认的缺口

本次只有 12 份真实 ZIL 源输入，保存在 `evidence/preflight196/cases`。使用既有 author193 CLI，SHA-256 为 `3830f924b7fa1223aeec88ecfd406f0a3012095ebd5be42a10aae0a7357ae33e`，精确构建 source 为 `f594c57665aaf34d0c1d54df11d5047d4d042f0d`，production 为 `03c20e5b05c8066ed703d6d00e3968e9ac134585`。已与原 clean `run-logs/193-author/final-build01/command/command.json` 和作者收据核对。

**这不是当前 main 的新构建。** 当前主线源码另作只读定位；其快照和相关文件摘要保存在 `evidence/preflight196/INSPECTION.json`。CLI 含 193，不包含 190 后续 Flow 运行语义；本次输入没有依赖运行时循环。

| 输入 | 当前实际结果 | 新能力的目标/安全边界 |
|---|---|---|
| 00 单角色 Fn 与 FnOnce 控制 | Success / Verify 1 次 | 现有单角色及 Fn→FnOnce 适配保持 |
| 01 同一个工厂产生两个 FnMut 实例，直接调用 | Success / Verify 1 次 | 同一具体类型可对应不同运行时环境 |
| 02 同 F，两个不同共享实参 | Expand Unsupported | 应接受 |
| 03 同 F，同一个环境传入两个共享角色 | Expand Unsupported | 应接受；角色编号不声称实参 apart |
| 04 同 F，两个不同独占环境 | Expand Unsupported | 应接受且保持各自状态 |
| 05 同 F，同一个环境作为两个独占实参 | Expand Unsupported | 开门后必须实际拒绝相交独占资格 |
| 06 两个类型角色 F/G，顺序组合 | Expand Unsupported | 应接受并保留调用事件顺序 |
| 07 F:FnMut 与 G:Fn 混合模式 | Expand Unsupported | 每个角色独立核环境模式与资格 |
| 08 同 F，两个不同拥有环境各消费一次 | Expand Unsupported | 应接受且每份责任结算一次 |
| 09 同 F，同一个拥有实参传两次 | Expand Unsupported | 开门后必须实际拒绝重复 Move/消费 |
| 10 嵌套 child 的两个调用点分别传 (a,b)/(b,a) | Expand Unsupported | 每个静态调用点有独立角色替换 |
| 11 第二个角色的 Desc 写与活跃共享视图同源 | Expand Unsupported | 开门后必须实际到达来源/借用拒绝 |

02/03/04/05/08/09/10 的精确拒绝为 `callable parameter must occur once as F, &F, &mut F or an owning State<F>`；06/07/11 为 `one callable type parameter with a direct bound is implemented`，全部 `E-UNSUPPORTED`。十个输入均没有执行 Verify，不能把 05/09/11 的前置拒绝记成 loan 或 Move 证据。

01 的实际 typed IR 中 `left: ClosureId(0)`、`right: ClosureId(0)` 对应不同 LocalId 和两次工厂调用。这已经说明**具体类型身份不是环境实例身份**；196 不能按 ClosureId 合并两个角色。

## 最小语义范围

建议首片承诺一个或两个同步、不逃逸的直接环境参数。覆盖同一 F 出现两次，以及 F/G 两个各有直接 callable bound 的类型角色；泛型 F 的重复出现仍须实例化为同一具体类型，运行时值可以不同。角色的环境参数仍按既有 F / &F / &mut F 模式传递。

每个角色使用已经支持的有限签名与当前调用资格，包括已有 Copy 地址和 193 显式 Copy slice 参数；多角色首片的回调结果与 helper 结果限 Copy。已有单角色 lending/State 路径保持；多角色借出结果、拥有 State 内的多角色投影、环境返回/逃逸、角色不明的合流、FFI、dyn、异步/并发和 CT 不由本片开放。

普通的「一个 F + Copy T」属于 194：T 不成为 callable 角色，不能从 T 的实例推导能力。若 194 先合入，196 在它已经固定的实例键上扩展 callable 类型绑定部分，保留其 Copy 类型参数身份与原专门化行为。这里没有 Copy T 探针或生产改动。

建议首片继续保守关闭「具有可达回边且含 FnOnce 角色」的合同，即使该角色无事件；按角色证明只在前缀消费一次、循环只调用另一角色，是后续额外消费分析，不能借 190 的单角色结论自动开放。有限无回边路径则分别计每个角色的至多一次消费。

## 必须共同改变的两个公开事实

当前 `CoreFunction.callable_parameter` 是单个 `CoreCallableParameter`；当前 `CallableParameterContract` 只有一个 `parameter` 和一个 `closure`，其 Finite/Flow 中调用事件只是 `CallableKind`。只放宽 Expand 的参数计数，会令后续来源、effects 和合同继续只覆盖一个实参。

最小的新公开模型是**函数级角色表 + 一份联合的调用语言**，示意如下，名称由 root 统一：

```text
HelperContract {
    roles: [RoleEnvelope],
    traces: Finite(paths) | Flow(graph)
}
RoleEnvelope {
    parameter: u32,       // 真实函数形参位置，而非 ClosureId
    closure: ClosureId,   // 此具体参数的类型/布局身份
    kind: Fn | FnMut | FnOnce,
    hold, call, writes, cleanup
}
CallEvent { parameter: u32, kind: CallableKind }
```

Finite 路径与 Flow 节点中的 `calls` 都变为有序 CallEvent 序列。角色表按真实参数位置严格排序、无重复，事件必须指向表内角色；角色的 ClosureId、模式、签名和 Owned/Borrowed Cleanup 必须匹配实际函数参数及独立检查过的闭包定义。非环境参数不进入角色表，所有实际环境参数都必须有对应行。表空缺、伪造参数位置、重复角色或不匹配种类不得靠默认值补齐。

选择真实参数位置作为公开事件身份，可直接映射 `Call.arguments[parameter]`，避免又引入一套与参数位置容易漂移的独立编号。位置只表明函数内角色，**不证明调用者两个实参互不相交**。

不能为两个角色各存一份互不相关的 traces：分支「只调用 a」与「只调用 b」必须保留为两个相关的函数路径，不能取笛卡尔积造出「两者都调用/都不调用」。也不能保留 190 的单一总计数，或改成各角色计数向量而丢失顺序；`a();b()` 与 `b();a()` 的次数相同，实际写、错误出口和消费顺序并不等价。

单角色旧合同提升为一行角色表、事件加上该真实参数位置，保持原语义和拒绝范围。已有 callable return 合同、环境布局、资源 route 与 lending identity 不因此改形。公共 Core 承载要能记录每个实际环境形参的 bound，重复泛型名对应同一已核 bound；不要把仅一个泛型名字等同于仅一个运行时参数。

## 三阶段独立重建义务

### Types 与专门化

Expand 的 `generic_callable.rs::template_with_state` 目前同时有泛型数为 1 和环境参数只出现一次两个门。应分离「类型参数 bound」与「实际环境参数角色」，由泛型定义检查 body，保留每个角色的源位置。`receivers.rs` 的 Core 转换、generic_check 的调用/转发、generic_helper_graph 的依赖提取都需要一致承载，不可只改第一道门。

Types 的实例计划当前为 `(TemplateKey, ClosureId)`，必须按泛型声明顺序包含全部 callable 类型绑定，防止 (F=id0,G=id1) 与 (F=id1,G=id0) 缓存碰撞。同 F 的两个运行时环境仍只有一个类型绑定，不能把运行时值塞进实例键。实例名称、查找和克隆成本要有确定顺序与预算。

每个实际调用先按原顺序准备所有参数，再提交已核转交；重复独占/拥有实参仍由实际来源和责任规则检查。类型层的单来源标记也要区分角色，不能把所有角色都写成旧值 1。未知或不同角色合流不得猜测任选一个；首片可明确 Unsupported，源代码用显式分支分别调用。

### Analyze

从真实参数与 SSA/Local 传播独立构造「来自哪个角色」的来源集合，区分本地工厂、未知和混合来源。每次回调事件必须由实际 callee 的来源确定，不从公开事件反推来源。

子 helper 已独立检查后，每个静态调用点按 child 的实际参数建立 child→caller 角色替换。10 中两个调用点的映射相反，即使两个角色 ClosureId 相同，也必须复制为不同位置并使用各自映射；只把 Return 接回该调用点 continuation，Trap 与无返回前缀保持 190 规则。

190 的 `Row.calls: usize` 和有限路径的计数集合只对单角色同种调用成立，多角色路径必须保留精确事件顺序或经独立论证等价的有界表示。不能把角色序列压成计数后仍宣称精确合同。

### Verify

Verify 继续只依赖公开 IR/base，从真实 TypedCFG、函数参数和独立来源关系重建整份合同。它不能调用 Types/Analyze 的求证函数，也不能用角色表自证 callee 来源。既有虚拟 `Body/Child` 位置和 BFS 可保留，但 Child 事件须经过**本静态调用点**的角色替换；独立构造结果逐项对比公开角色表和联合 traces。

`borrows/loans.rs` 当前 helper Call 的 held 取自唯一 `arguments[contract.parameter]`，必须考虑每个角色及全部当次显式实参。全部窗口进入 active；授予当前角色的重借/自身窗口排除资格时仍保持角色范围，不能把所有角色窗口并成一个可豁免集合。即使最后一次调用也不能漏掉第二角色的冲突。

Analyze/Verify 两个 resources 消费者当前也只投影一个 `contract.parameter`。应按各角色的实际事件存在性和该角色具体环境，分别把写上界代入实际来源。一个角色无调用不能抹掉另一个角色的无返回前缀写；不同形参名或 F/G 名称不制造 apart。Cleanup 按角色对应的实际 Value/Borrowed 责任保持守恒。

## 预算与公开身份

角色列表、每行 hold/call/writes、每个角色事件、角色来源集合、实例键、child 替换表、复制/重定位、有限序列拼接/比较、队列和公开合同对比均需在对应分配/遍历之前计费。总节点、总事件、总边与角色元数据累加使用 checked 算术，有限 trace 数和完整事件长度均受已有额度约束，默认额度不增加。最坏情况下多角色顺序会产生更多不同序列，超额明确 ResourceLimit，不截断成成功。

静态 `max_call_depth` 继续计 helper 依赖链；角色个数与运行时回调次数不能伪装成该深度。阶段别 N/N−1 与 fresh retry、两次 static child 复制、分支路径膨胀都应有实际测量。

这是公开合同的形状变化，必须由 root 发布下一 schema/Wire 与相关合同、模块、已核接口指纹域。身份包括角色顺序、参数位置、ClosureId、模式、各 envelope、每个事件的角色和种类以及完整图边/出口。旧 Wire/接口不能静默接受。Driver JSON 从 VerifiedModule 输出全角色联合合同；None 与已核空导出集继续区分。

## 最小行为写集与机械影响

以下是依照当前源码单角色读写点定位的建议，不是提前获取全目录行为修改许可。

| 责任块 | 最小必需路径 |
|---|---|
| root 公共表示、Wire、身份、规范 | `zil-base/src/lib.rs`；`zil-ir/src/lib.rs`、`wire.rs`、`program.rs`；`zil-driver/src/contracts.rs`；root 管理的 spec/data/task |
| 前端角色提取及 Core | `zil-expand/src/generic_callable.rs`、`generic_check.rs`、`generics.rs`、`generic_helper_graph.rs`、`receivers.rs` |
| Types 绑定、实例和实际事件 | `zil-types/src/helpers.rs`、`helper_plan.rs`、`helper_flow.rs`、`closures.rs`、`inference.rs`、`lib.rs`、`program.rs`；`callable_fields.rs` 只负责保留旧单角色 State 路线和明确新边界 |
| Analyze 自有来源、联合轨迹和效果 | `zil-analyze/src/helpers.rs`、`helper_flow.rs`、`budget.rs`、`resources.rs` |
| Verify 自有重建与当前调用资格 | `zil-verify/src/helpers.rs`、`helper_flow.rs`、`resources.rs`、`borrows/loans.rs`、`lib.rs` |
| 实证与有限 consumer | 新 `impl196_*` source/raw/Wire 测试、`examples/mainline/*196`、有界运行脚本及独立 review |

表中 crate 路径均以 `crates/` 为前缀。现有普通函数多参数 lowering、各闭包自身单环境入口与 ABI 看起来可复用；这是源码定位后的推断，尚未由多角色程序构建证明。`zil-lower`/`zil-backend-llvm` 不是先行行为写集，只有实际编译暴露独立根因时再申请精确增项。公共 Rust 类型变化造成的构造器/fixture 迁移按精确编译错误机械处理，不混入其它接受性改变。

## 建议实施顺序与必需验收

1. root 先固定公开角色表/联合事件合同与规范，机械提升旧单角色；多角色语义在生产未就绪时仍明确 Unsupported。迁移必须保留原单角色源输入和旧安全反例，重跑 188/189/190/191/193 与已完成 194 的相关控制。
2. 前端/Types、Analyze、独立 Verify 按分离写集并行施工。所有三阶段到位后，原 12 输入再原样验证；02/03/04/06/07/08/10 由前置 Unsupported 变为实际通过，05/09/11 到达正确安全门，不能改输入绕开。
3. raw/Wire 至少攻击：交换两个同 ClosureId 角色事件、删除/复制角色、事件指向非角色参数、伪造模式/cleanup、仅擦第二角色写、协调篡改 parent/child 角色、两调用点共用错误替换、重复实参、改出口/回边/无返回前缀、缺预算和旧 Wire。每个攻击有相邻正控制，归因到实际拒绝门。
4. 原生 O0/O2 使用打印和独立状态计数区分两个同类型实例，验证嵌套交换、分支只调用一角、FnOnce 各自消费和零调用清理、错误前缀准备/清理顺序；再与 193 文本/缓冲参数组合。各平台单列，尚未执行的 Wine/Windows 不计通过。

本预检只有源面 check，没有新增原生运行、分配失败或 raw/Wire 新语义验收。它确认一个真实前置缺口并提供可派工的最小公开模型；不证明 196、完整 1.4、ZUI 或 Rust 四目标已经完成，也没有编译速度/内存比较或产品净突破结论。
