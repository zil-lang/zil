# 0125 — 工厂内部组合普通回调 helper

本单解除一个真实语言组合障碍：自包含的 `impl Fn/FnMut/FnOnce` 返回工厂可以在正文内调用既有的非逃逸 callable helper，准备拥有状态或预热将返回的 handler。既有直接调用、实例化、调用轨迹和公开 Hold/Call/Cleanup 合同承担这项组合；没有新增特殊 ZUI 原语。

受测源码是 `126ba2486f31945bd9a7a2ef269d1f1952667791`。基线是 `ff0ddb7e59b2b0577c35b92e74b895c86f20b218`，schema/wire 17。作者限定验证通过；独立复审、集成和正式验收由集成方处理。

## 实际语言效果

基线的 `make → apply(&init, seed) → return move closure` 在 Types 以 Unsupported 拒绝，Verify 尚未执行。新源码通过实际四模块公开接口进入 Analyze、独立 Verify、LIR、LLVM 和原生执行。新增消费者位于 `examples/mainline/component_factory_helpers/`：

1. 工厂把拥有初始化资源的 Fn 闭包传给普通共享 helper，得到初始状态。
2. 将状态放入拥有 `Model` 的 FnMut handler，并在工厂内通过另一个可变 helper 预热同一个 handler。
3. 返回 handler；调用者保存两个工厂实例，交错处理三次事件。
4. 初始化临时资源在各自工厂返回前清理，两个返回环境在调用者退出时分别清理。

O0/O2 都输出 `IIRR63\n`；实际 CLI 文件加载和导入解析另执行一次，输出相同。`I` 和 `R` 是真实 Drop 调用输出，63 来自独立状态的实际更新。其他正例覆盖 Fn、FnMut、FnOnce 初始化，返回只读、可变和消费环境，未调用环境清理，以及成功/错误返回的 Native main 退出码。错误程序使用真实 `fn main()->bool!i32`，不会把标量测试打印器误当语言 ABI。

## 实现与独立核验边界

接受性提案 `69cf71eb` 在生产改动之前；首候选 `8fa6c79d` 只移除表层误拒，暴露了 helper 模板尚未安装的准备顺序缺口。扩展时序提案 `68ea30da` 仍在对应生产修复之前。最终生产改动仅限 Types 的 `lib.rs`、`program.rs`、`helpers.rs`。

helper 模板和已计费签名快照移动到整个 factory prepass 之前，只安装一次。私有 helper lookup 在**全部**工厂检查结束前保留未解析 opaque 结果。工厂内直接调用另一个工厂仍拒绝；helper 若间接调用当前工厂或其他尚未开放的 callable 工厂，也在 Types 拒绝。这个结果不依赖工厂的声明或模块检查顺序。普通函数中的 helper 在 prepass 完成后仍可调用已解析工厂、持有并清理返回环境，原有合法组合不被全局禁掉。这个兼容控制不包含在泛型 helper 正文再次直接调用刚取得的局部环境；独审该近邻仍触发既有 Expand/E-GENERIC-ARGUMENT，是另一个尚未解除的组合限制。

所有工厂完成后，只把私有表相应结果槽更新成已经检查出的具体 `Type::Closure`，不深复制第二份整表。`Rc::get_mut` 的唯一性来自具体生命周期：唯一额外的表 Rc 是 `instantiate_helper` 的局部变量；它及其借用的 Builder/Imports 都在该同步实例化返回时释放。模板的 Rc 指向 Context，Context 只保存模块、导入别名和枚举；Pending、TypedFunction 与导出合同不保存表 Rc。两个入口只在整个工厂遍历结束后同步，所以同步点没有悬留克隆。没有 helper 的程序在同步遍历外直接跳过，避免新增全函数扫描。

helper 和 factory 仍产生普通 Call、独立环境身份和原有公开合同。新测试直接核对 factory CFG 中存在指向生成 helper 的 Call，而非正文替换。raw/wire 入口各接受原始候选；分别删除 factory cleanup、删除 helper traces、伪造 factory FnOnce 模式时，两入口都以 Verify/E-VERIFY-CALLABLE-CONTRACT 拒绝。每次篡改重新绑定候选指纹，因此拒绝来自语义核验，不是旧指纹不匹配。Analyze、Verify、后端和公共 schema 没有改动，也没有共享 Types 的求证结果作为权限。

返回借入局部捕获、工厂递归/转发、FnOnce 重复调用、错误调用模式仍拒绝。两个模块同名工厂的隐藏身份保持不同，逆转模块输入顺序仍通过。当前每条 helper 路径最多两次借入调用的限制保持；另一个三次调用探针作为原基线拒绝原件保存，交给后续独立工单，未在 125 中改写。

## 预算与限定验证

初始快照仍通过既有 `reserve_signatures`、模板快照和实际实例化收费。新同步在修改前按名称字节数加固定工作收费；结果已限定为常量大小 Closure 身份。默认额度不变。因为初始私有表现在含 opaque 类型，工作计费不能假称与旧低额度接受边界完全一致。

实际新组合的最小成功工作额度为 Types 426、Verify 4048；425 和 4047 分别以 ResourceLimit 拒绝。Types 路径同时经过安装、工厂内实例化和结果同步，旧额度没有被偷偷调高。这是正确计费证据，不是编译提速证据。

冻结源码执行的 76 个限定测试组均通过：

| 范围 | 组数 |
|---|---:|
| 既有 helper 065 | 17 |
| 返回 callable 069 | 18 |
| 保留回调真实消费者 070 | 1 |
| Package 捕获原生消费者 079 | 4 |
| 拥有字节源码/原生 099 | 7 + 7 |
| callable 名义类型 121 | 5 |
| CT/runtime Drop 122 | 6 |
| 本单工厂/helper 组合 125 | 11 |

新增 suite 对八种原生程序各跑 O0/O2，共 16 次，另有一次实际 CLI 消费者执行。新增源反例八次、模块身份正例两次、普通调用者兼容控制一次；raw/wire 原始通过两次、篡改拒绝六次。fmt 和 `zil-types`/`zil-driver --tests` clippy 无警告通过。没有把重复执行当成独立缺陷，也没有运行作者无权安排的全量正式验收。

## 原失败与证据

`evidence/impl/125/acceptance.json` 绑定受测提交、完整原件归档和逐文件摘要。归档包括基线探针、各候选源快照、原始 stdout/stderr、最终源与阶段产物、实际机器程序、公开接口、raw/wire 和预算边界。

首次 development-001 的 runner 没有设置保留目录，只有当时的 stdout/stderr、改动快照和精确源码留存；单独的 development-001-replay 使用同一源码重放并保留阶段产物，明确不是原首轮产物。后续 development-002 的唯一失败是夹具用了尚不支持的 `42i64` 字面量后缀；development-003 的唯一失败是夹具把错误联合返回交给仅支持标量的命名入口打印器。两次都按原失败保留，修复在测试中，没有为夹具放宽语言规则。development-004 十组通过，最终候选再增加可运行消费者并消除无 helper 路径的额外扫描后执行上述完整限定验证。

## Rust 四目标与剩余工作

安全方面，本片把已有 helper 与返回环境合同真正组合，并有最近源反例和独立 raw/wire 拒绝证据；这不等于完整语言安全。表达方面，组件作者能把准备和预热逻辑放在普通 helper 中，解除已实证的组合误拒，但维护成本优于 Rust 未测。运行和内存方面，只有输出、退出码、清理顺序与原有表示证据，没有新增相对 Rust 的成本测量。编译成本方面，这项修复没有提速声明，安装/同步的真实成本已计费。

本片的实质进展是可运行的普通语言组合，不是新图定理或产品级 Rust 净突破。一般递归工厂、借入逃逸、动态外部回调生命周期、helper 更一般控制流和完整 ZUI 框架仍有边界。B0 Doing、完整语言 NotSatisfied、产品 NotRun 与既有量化门保持。
