# 下一阶段 ZUI 语言施工建议

审阅基线：`712a9602797891083fe3f549381e1e12ab34dd02`（161、162、163 已合入）。本稿仅安排现行规则的实施，不增设语言规则、产品门或性能结论；未修改主树，也未执行新增测试。

## 建议次序

**先补拥有值的组合与错误传递，同时推进具体闭包组合；再接一般拥有元素容器和回调集合。真实头地址独立推进，不作为所有功能的共同前置。** 163 已提供普通资源 Desc 方法及显式 give 消费，直接资源方法不必等待 `&Self`。Package 的借入更新、稳定外壳地址等消费者仍必须等待真实头地址的完整实现。[1][2]

### 四类缺口与实施边界

| 方向 | 当前实际边界 | 建议第一交付 | 现行规范与跨阶段要求 |
|---|---|---|---|
| 拥有 `E!T` 载荷 | 类型解析要求 E/T 都是 Copy；`?` 用 `ReadErrorUnion` 提取；LLVM 当前是 tag 加两个固定载荷槽 | 先用明确拥有的完整 Package 作为一个分支，再覆盖两分支均拥有；跨函数构造、传播、未取出销毁完整贯通 | 现有 §7、§15.1 已规定注入及错误出口清理。须协同 IR/Wire、Analyze、独立 Verify、Lower/LLVM；读取 Copy 载荷与取走唯一责任必须有可核验的区别，非活动分支不能产生清理责任。[3] |
| 一般拥有元素容器 | 数组、元组限制 Copy；Package 仅直接 Managed 与 Copy 字段；ByteBuffer 仅 u8，尚无通用拥有元素存储 | 先做 `[P; N]` 等有限同质拥有聚合及逐项部分初始化/移动/清理，再扩到有明确存储和元素责任的动态容器 | §3 的 One/Many、§7–8、§9.5 已给语义边界。动态部分需要公共存储/操作合同、元素布局及清理 ABI、来源/失效与独立 Verify；不能把 ByteBuffer 的字节接口换个 T 就开放。[4] |
| 闭包保存／回调集合 | 已能保存单个自包含 opaque 环境；其参数结果仍限 Copy，工厂转发/递归返回被拒绝，Closure 不能作为一般捕获/聚合值 | 先具体环境的转发与再捕获、拥有单槽保存/替换；随后同质集合；异构集合最后接显式存储与完整 dyn 合同 | §9 已允许这些能力。能用现有 Closure/CallableReturnContract 完整表示的小步可先实施；嵌套责任、持久字段、擦除或新结果模式必须同步公开合同与 Verify/ABI。不同闭包表达式不能自动并成一个 opaque 类型；禁止隐式装箱。[5] |
| 真实头地址 | `&Resource`/`&Package` 在类型解析中明确拒绝；BorrowLocal 的普通地址路径只接受 Copy/Closure，未证明拥有外壳借入 | 先真实局部外壳的同步共享借入，再独占借入及投影/接收者；后续才纳入跨函数来源、环境 Hold 和长期稳定存储 | §2.2、§4、§8 已规定地址窗口与外壳移动约束。须协同 Address 类型准入、真实位置/资源状态、Verify、来源合同与机器指针 ABI；`&Self` 不能降为 Desc，也不能靠更换类型标签取得资格。[6] |

## 最多三名施工者的安排

公共前置由 root 单写：先收齐三个方向的具体接口差分和最近反例，固定非 Copy 载体在聚合/联合/环境中的转交、初始化与清理表示，以及预算和序列化兼容方式。**不要求一次实现全部通用能力；每次交付固定一个闭合的有限类型集合。** 共享类型/指令/接口注册按实际变化更新，不能承诺当前 wire 直接兼容扩大的语义域。[7]

直接 Managed/ByteBuffer 进入 `E!T` 分支前，须明确既有 create/give/from 结果模式如何在该分支的类型与导出接口中表达；当前实现只在整个函数返回类型外包 Managed，不能据调用处需求猜测联合内部的责任。第一段用 `package P { manages ... }` 保持源面类别明确。动态容器的新可信原语、异构存储接口同样先提交具体合同；本稿不发明语法、内建 API 或失败签名。[1][3]

| 施工者 | 第一段可并行施工 | 后续接续／依赖 |
|---|---|---|
| A：错误与借入 | 拥有 `E!T` 的完整纵向切片：注入、分支取走、`?`、返回准备与清理；独立 Verify 重新建立真实责任 | 错误切片稳定后接真实头地址。先同步局部借入，再开放导出来源与 Hold；与 B/C 的实际调用组合后验收 |
| B：拥有聚合与容器 | 有限同质元素、部分初始化前缀、取走/补入/替换/整体移动；采用公共载体表示 | 接动态存储、容量失败、逐项清理及 One/Many 汇总；公开失败接口与 A 的拥有结果组合。若消费者需要借入 Package 外壳，与 A 的地址切片对接 |
| C：具体闭包与保存 | 工厂转发、具体环境再捕获、单个拥有回调槽、零次/一次/多次调用后的替换与销毁 | B 的元素载体稳定后接同质回调集合；异构集合再引入规范已有的显式 Box/arena/调用者槽之一及完整 dyn 合同。拥有回调参数/结果与 A 对接 |

文件重叠按功能区域分配：A 主写 `errors` 与后续 `addresses/borrows`；B 主写拥有聚合/容器；C 主写 `closures/helpers/retained`。三者各自提交 Analyze、Verify、Lower/LLVM 的配套实现；`zil-ir`、wire、公共接口及共同类型分类由 root 串行合并。不能仅派一个前端实现者、等最后才补安全核。

## 每段最少交付与关键反例

- **E!T：** 成功/错误两个分支、连续两次 `?`、后续实参失败、未提取销毁；拒绝重复取走、错误 tag 的责任、错误出口漏清理。先保留可解释的内联表示，无需同时压缩联合布局。
- **容器：** 第 k 项准备失败、空/部分/完整销毁、元素移出补入、增长失败与活跃窗口；拒绝把 Many 当 One 强更新、重复元素责任、移动/重分配后使用旧窗口。
- **回调：** 未调用销毁、Fn/FnMut/FnOnce 保存后调用、替换先准备新值再结算旧值、活跃 Call 与旧 Cleanup；拒绝丢失 Hold/Call/Cleanup/Resume、消费回调重复调用及异构环境伪装同型。§9.5 的 HandlerSlot 延期结算只能来自明确已核协议，不作为普通赋值默认行为。
- **头地址：** 共享与独占别名、Package/环境内头投影、跨调用和错误退出；拒绝借入期间 give/drop/搬迁、以重建同名头恢复旧借用、将短命外壳地址逃逸。

每段都需要真实源码、raw/wire 最近对抗例和 O0/O2 清理证据；四模块消费者继续组合“构造—保存—调用—替换—失败—关闭”。先让具体拥有类型闭合，再推广到泛型；不用测试数量宣布 ZUI 前语言缺口已经补齐。

## 依据索引（均为上述基线）

1. `_zil_design.md`：ZIL-MODES（§2.2）、ZIL-ACCESS（§4）、ZIL-PACKAGE（§8）、ZIL-CAPTURE（§9.4–9.5）、§15.1/15.4。
2. `crates/zil-types/src/aggregates.rs`：`resolve_type_method`/`method_call`；`crates/zil-types/src/closures/package_effects.rs`：Package 方法按值转交。
3. `crates/zil-types/src/aggregates.rs`：`Type::ErrorUnion` 分支、`resolve_indexed` 的 `raw_result`；`crates/zil-types/src/errors.rs`：`try_expr`；`crates/zil-types/src/resources.rs`：`has_cleanup`/`drop_owned_local`；`crates/zil-verify/src/lib.rs`：`ReadErrorUnion` 与类型检查；`crates/zil-backend-llvm/src/emit/errors.rs`。
4. `crates/zil-types/src/aggregates.rs`：数组/元组 Copy 限制；`crates/zil-types/src/packages.rs`：`supported`；`crates/zil-types/src/byte_buffer.rs`：`parameters`/`add_definition`；`crates/zil-verify/src/byte_buffers.rs`；`crates/zil-lower/src/lib.rs`：`package_type`。资源发现目前也只扫描有限直接字段，推广载体时必须一起补全递归遍历。
5. `crates/zil-types/src/retained.rs`：`validate_factory`/`factory_return`；`crates/zil-types/src/inference.rs`：`scalar`；`crates/zil-ir/src/lib.rs`：`CallableReturnContract`/`ClosureDefinition`；`crates/zil-verify/src/retained.rs`。
6. `crates/zil-types/src/aggregates.rs`：`Type::Address` 分支；`crates/zil-types/src/addresses.rs`：`address_place`；`crates/zil-verify/src/borrows/loans.rs` 与 `resources.rs` 的窗口、位置和真实责任检查。
7. `spec/interfaces.json` 的 `shared_change_owner`、`crates/zil-ir/src/lib.rs`/`wire.rs`/`program.rs`、`AGENTS.md` 第4–8条；公开 schema 的变更与独立审核仍由集成方协调。
