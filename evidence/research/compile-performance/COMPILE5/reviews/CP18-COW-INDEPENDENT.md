# CP18 Cow 独立静态审查

- 审查者：`/root/ct_opt`（未参与 CP18 实现）。
- 基线：`abe786558be7e76f14b4b2398e96890c3b408e32`。
- 被审提交：`8cd1a412e5ab260843469a3d9b25125a9bed5140`。
- 范围：该提交的 Resolver/Cow 改写；Core 节点字段保真、类型相等、返回所有权、集合顺序与复制边界。
- 结论：**静态审查通过，未发现阻断性正确性缺陷。** 本审查没有运行 Cargo、动态测试或性能负载，不代表运行验证通过或已证明加速。预算与首个诊断的独立审查由 `/root/industry_audit` 负责。

## 1. Core 节点和元数据

逐项对照提交前后 `crates/zil-types/src/aggregates.rs` 的重建逻辑及 `crates/zil-ir/src/lib.rs` 的结构定义：

- `CoreProgram` 保留 root、source_fingerprint、模块顺序；`ProgramModule` 保留 imports 和 exports。
- `CoreModule` 保留 schema_version、module_id、source_fingerprint、functions、aggregates、enums、unsupported。
- `CoreFunction` 保留 type_parameters、state_parameters、callable_return、id、name、callable_parameters、foreign、parameters、return_type、result_mode、body、receiver、span。状态绑定、可调用参数/返回、普通参数、receiver 的各项索引、名称、kind、mode、implicit 和 span 均被保留。
- 聚合保留 id/name/kind/teardown/fields/span；字段保留 name/type/public/manages/span；枚举保留 id/name/variants/span。
- 已逐一核对全部递归语句、表达式和模式变体；重建节点保留 id/span。分支合并后仍正确区分 Block/Unsafe、Ascribed/IntegerConvert、Unary/CompileTime/Try、Tuple/Array。
- Match arm 的 span、命名字段及 Sequence binding 的名称和顺序、Member 成员名、Closure 的 move_capture 与参数 mode/span、Slice 的可选边界均未遗漏。无须改写的叶节点借用原值。

函数处理期间借用原始 callable/state 头，与旧代码处理之前先克隆这些头的含义一致；没有误用已经规范化的中间头替代原输入。

## 2. 类型相等没有忽略语义字段

`Type` 使用完整派生的 `PartialEq/Eq`，没有自定义的近似等价关系。比较包括 Record/Package/Enum 的名义名称及有序内容、Address.mutable、Array.length、Opaque.identity 和 hidden、ClosureId、函数签名及所有包装层。

`type_cow` 先执行原 `self.ty`，仅在完整返回值等于原类型时借用原类型。因此相等比较本身没有删掉原解析/检查，也没有把不同名义身份或拥有模式当作相同。函数参数和 receiver 另外比较最终 mode；因此即使类型相等，mode 发生变化也会生成拥有所有权的新节点。

## 3. 推断与解析的所有权组合

`resolve_program_shapes_borrowed` 的三个有意义分支正确：

| 闭包推断结果 | 形状解析结果 | 对外返回 |
| --- | --- | --- |
| Borrowed 原输入 | Borrowed | Borrowed 原输入 |
| Borrowed 原输入 | Owned | Owned 解析后的程序 |
| Owned 推断后的程序 | Borrowed / Owned | 分别移动推断后的程序 / 解析后的程序并返回 Owned |

关键的 `Owned(inferred)` 加 `Borrowed(_)` 分支返回 `Cow::Owned(inferred)`，没有借用逃逸，也没有误退回最初未推断的输入。公开的原 owned API 仍通过 `Cow::into_owned` 兑现拥有所有权的接口。单模块 resolver 使用同样的 Borrowed 时移动原 core 的处理方式。

## 4. 集合惰性复制和递归边界

`map_cow` 按输入顺序处理所有元素。首个 Owned 元素出现时只复制一次此前的原始前缀；此后 Borrowed 元素按顺序 clone，Owned 元素直接 move。各调用点的 Borrowed 都指向对应的原元素，因而以原始前缀重建不会丢失已完成的转换。`map_optional` 保留 None/Some，Some 子值改写时保留包装。

已经改写的子树向上层通过 move 转移，没有在每层祖先重新 clone。未改写子树只在第一个确实需要拥有所有权的父节点中整体复制；多个兄弟节点改写不会再次复制已构造的前缀。单个失败会丢弃当前临时结果，不会暴露部分更新。

这一判断仅针对新增 Cow 重建逻辑；不声称旧类型展开、声明索引或已有临时类型 clone 的成本已消失。`type_cow` 仍构建原 `self.ty` 的结果，并新增一次完整 Type 比较。其深度检查与类型解析仍先执行，新增比较未引入不同的语义递归分支，但大型复合 Type 的比较是实际额外工作。全改写输入和复合类型密集输入仍需要测量；本次静审不作普遍加速或零回退的结论。

## 5. 动态验证边界

该 Cow 提交新增的两个用例覆盖原程序借用、恰在中间发生的改写、前后未改函数/模块、拥有模式、输入不变和 fresh budget 请求。它们没有直接断言以下组合：

1. 闭包推断返回 Owned，形状解析无需进一步改写，最终仍保留推断后的完整程序。
2. 闭包推断返回 Owned，后续形状解析继续改写，两个阶段的更改及元数据同时保留。

建议在整体验证中保留现有 `borrowed_inference_cp15` 和 `closures_frontend` 的通过记录；若新增精准用例，由作者或整合者断言上述两个组合的完整输出及原输入不变。这是覆盖建议，不是本审查发现的语义错误。针对多处改写的性能比较应保留大型复合类型/全改写阴性组，防止仅根据未改写的标量项目推断普遍收益。

## 执行记录

本次只读取精确 git 对象及测试源码，并运行 `git diff --check abe786558 8cd1a412e5ab260843469a3d9b25125a9bed5140`（无输出，通过）。未修改 CP18 作者源码；未执行 Cargo 或基准。
