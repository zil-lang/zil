# 下一语言主线：同步拥有事件的 callback 调用闭环

建议下一批只设一个主目标：让普通拥有的 Event/Reply 可以穿过具体闭包和公共泛型 dispatch 的调用边界，保留实际责任、来源、失败准备与清理。先用真实同步 API 驱动同一纵切，完成后再决定 F 原样返回是否与实际消费者合批；不按 Unsupported 数量拆成清单式小功能。

## 身份与三次实际检查

只读规划，没有修改生产、规范、工单或正式测试，没有 Cargo 构建或 native 执行。CLI 是已封存的 243 作者 CLI，SHA256 `09b619241e50d6f06e7bb9b20047c134c045ea1ff50cace6085c67c27d409e51`，构建源码为 `9411e92db84fa9a71c40dfbe5cc0e67f1cfaa1f4`；分树当前 HEAD `baad307787ebd9a4c007ed696d9a8c7683728215`，检查前后 clean。本轮不把它声称为尚未重建的 root 最终共同 CLI。root86f7dc3c 与该受测源码的 closures.rs、inference.rs、generic_callable.rs 三处相关门没有源码差异。

三个源码全部逐字复制已保存 Q 预检，而非新造语法；预期在运行前写入 PLAN.json。原 Q 的完整报告从仓库 `evidence/planning/language-next-Q/owned-callback-preflight.tar.gz` 只读解出。新原始源、argv、stdout、诊断及阶段产物在 `checks/`，汇总 RESULT.json。

| 原源码/API | 本轮实际最早边界 | 含义 |
| --- | --- | --- |
| `dispatch<F:FnOnce(Event)->Reply>(f:F,event:Event)->Reply`，Event/Reply 各拥有 ByteBuffer | Types Unsupported，E-UNSUPPORTED：closure parameters require a finite Copy type or one Copy/slice address layer；Verify0 | 拥有 callback 入参是真缺口。此源在入参处先停，单凭它不宣称已经执行到 result 核验；拥有结果的独立 Q 原源另有 Types Unsupported，当前 supported_signature 的 Copy/lending result 门仍在。 |
| `keep<F:FnOnce(i32)->i32>(f:F)->F{return f}` | Expand Unsupported，E-UNSUPPORTED：escaping callable result is not implemented；Verify0 | 裸 F 原样转交是真缺口，和“返回新闭包”的既有工厂不同。 |
| `State<F>` 完整拥有转交、调用其 Copy callback 后进入 `Store<State<F>>` | Success/Verify，实际核验1次 | 保存/调用/普通封装转交和已有有限回调集合不是统一阻塞。已有 Q 的 O0/O2 原生通过仍是旧身份证据，本轮0次原生。 |

## 已能自然表达，勿再派相同功能

- 普通函数接收/返回完整 Product、嵌套 Store、错误值；普通拥有 Sum 的有限实际分支、解构、替换与清理已有本批 239–244 交付。后者仍待 root 最终共同候选验收，不能从作者通过推成 main 已完成。
- 有限 State<F/G>、具体回调字段、同型不同实例、已有 callback Store、泛型 StoreElement 库、ID/父ID、准备新值后替换、关闭和失败路径可组合。固定类型家族分表加 ID 能做事件分发，不需要先做 dyn。**StoreElement/capture 各有原资格门**：本批 Sum 支持不等于任意 Store<Sum>、Product<Sum> 元素或 Sum 捕获已开放。
- 普通同步 defer 已有退出履行；手写函数、静态 nominal trait、Copy 泛型、有限 #if/#for 和非泛型 struct/alias/Unit-Tuple enum 生成可用。无须先实现一般 Syntax 才写同步库 API。

## 最急边界与最短依赖

| 优先级 | 最小 API/来源合同 | 真缺口与边界 |
| --- | --- | --- |
| 主目标 | `dispatch<F:FnOnce(Event)->Fault!Reply>(f:F,event:Event)->Fault!Reply { return f(event) }` | Concrete closure 和泛型 helper 同时承接拥有参数/结果。使用普通已核完整类型；从 ByteBuffer Product 到本批 Product/Store/Sum 复用真实资格，不借机扩大 capture/StoreElement/FFI。Fn/FnMut/FnOnce 描述环境，不是每个 Event 的所有权：Fn 也应能每次消费一个独立新 Event。 |
| 紧随的横向能力 | `keep<F:FnOnce(i32)->i32>(f:F)->F` | 需要公共“实际输入角色 → 返回同一环境值”的来源关系。现有 218 工厂只返回 grounded 新自含环境，不能用同 ClosureId、包装新闭包或空 callback envelope 冒充。先冻结自含拥有 F，再明确带 Hold/借入环境转交范围；保留声明 FnOnce 适配，不按实际较强 Fn 洗回权限。已有 State<F> 转交是当前可用库路线。 |
| 声明式入口下一主线 | `syntax use zui.syntax as ui; ui!{...}` / `fn component(...)->Syntax<...>` | 通用 Type/Syntax、#emit、反射和生成 trait/impl 确实尚未闭环；有限 #for 不是该功能。它阻塞声明式 ui! 与工具映射，不阻塞上述手写同步 API。本轮未新增该探针；结论据 §16、§18 及已存217原探针，不伪装为最新 CLI 重跑。 |
| 独立系统主线 | `register(callback, userdata)` → stop admission → unregister/drain → reclaim | §12.3/12.4 当前只有 noescape/static 无环境回调。需要稳定注册根、注册失败、重入、进入登记线性化、active+no-future、晚消息/唤醒入口与最终回收。普通 ZIL Subscription 包可拥有 ID/资源并同步 close，但不能因此证明外部已不再访问 userdata。与 Suspend/取消/跨线程一起须独立真实平台合同，不能靠 unsafe 指针/全局表或把 active=0 当 no-future。 |

一般关联项/lending/dyn 留在真实 API 需要时再推进。已有 nominal trait 和同质 Store 不等于开放异构组件，但无需为本批 dispatch 先做通用 trait 求解器。

## 主目标怎样构成可验收闭环

保持一个真实的三模块 API：model 定义拥有 Event/Reply/Fault；dispatch 提供公共泛型调用；consumer 用 callback 接收 Event、转移叶到 Reply 或返回拥有 Fault，并继续传给普通业务函数。先固定源和正常/错误/未调用/后续参数准备失败的清理字符 oracle，再实现源 → 公开合同 → Analyze/独立Verify → 两后端原生。

必须一起覆盖：参数先准备后转交、每个 owner 仅消费一次；结果先进入真实槽再清 FnOnce 剩余环境；Fn/FnMut 重复调用每次取得独立 Event；callback 未调用和后续 `?` 失败只清已准备责任；结果不得借已销毁环境；重复 Event、旧别名、缺失/重复清理与 raw/Wire 错签名/来源为近邻反例。callee 环境责任和每次 invocation 的参数/结果责任必须分开。

当前实体 CallClosure* 已有 arguments/result SSA、ClosureDefinition 完整类型，机器 builder 可复用；这只是施工起点，不推定无需公共表示变更。公共 helper envelope、参数/结果责任和 route 来源须双核独立闭合，不能只移除 Types 的 Copy 门。预算仍按当前实际请求先收费，源/raw/Wire/机器与失败清理完整后才能称此目标完成。

范围判断：有限库表达增加有实证；完整语言/ZUI、Rust安全/运行与RSS/维护/编译成本四目标合取及产品净突破仍未证明。本次只读预检不升级其状态。
