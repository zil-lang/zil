# ZUI 开工前语言闭合矩阵与 217 同步消费者

审计者：`/root/zui_gap_audit`。日期：2026-10-29。只读生产源码；217 新源是语言组合消费者，不是 ZUI 框架或完整语言验收。原始审计与正式 217 执行分别记账。

## 结论

**不能把 214–216 完成写成“所有 ZUI 语言前置已全部清零”，也没有实证支持立即把嵌套 Store、拥有型 Option、通用 dyn 或完整 CT 全部升为同步框架开工的硬前置。** 已有语言能够表达运行时数量的节点、具体状态回调、索引/ID 关系、普通泛型容器转交、独立删除与错误清理。当前真正尚须关闭的最小事项，是 214–216 的完整纵向验收，以及这些能力在明确同步框架消费者中的共同闭环。后者是待验证的组合问题；遇到具体不能表达的合法消费者后，才能据此开新的语义增量。

本次新增实证尤其排除了一个容易误判的“硬缺口”：无需 `Store<Store<...>>`、拥有 enum 或任意 Product 元素，普通 `Node<F>` + `Store<T:StoreElement>` 就能删除中间节点、立即清理该节点，同时保存另两个真实 FnMut 环境的状态。本轮 O0/O2 均独立 Verify 1 次、机器退出 0，输出 `B!AC`：B 在事件标记 `!` 前被销毁，A/C 在后续调用后关闭。这个控制只证明现有表示可行，不是任意图、失败全覆盖、效率或完整 ZUI 的验收。

## 判断依据和范围

- `_zil_design.md` §8.1 要求逐个开放拥有组合，明确不是所有递归组合立即受支持；§8.1.1 和 §9.1.2 明定目前 Product 范围。源形状不被当前 profile 接受，不自动等于所有 ZUI 都无法表达。
- §18.2 把 `syntax use` 定义为显式选择的可替换表面层，且明确普通未使用扩展的代码不启动扩展 VM。因此 `ui!` 的真实生成链是声明式入口的前置，并非所有普通同步代码的前置。手写消费者不能替代该生成链的完成证据，但声明式未完成也不能阻止已有普通语法合法程序开始构造同步库。
- `docs/NEXT-LANGUAGE-178.md` 的“受限 ZUI 原型可以从哪里起步”明确认可普通 State、同质容器与同步回调路线；`docs/LANGUAGE-195.md` 的“优先剩余项”第 5 项明确外部保留/异步/并发不一概作为同步手写最小入口前置。它们是施工范围说明，不覆盖规范终态。
- `_zil_design.md` §12.4 的同步 noescape/static callback 与 retained userdata 是不同合同。现有 Win32 消费者采用前者，不能据其成功声称后者完成。
- W2 的生成同核、dyn、图演进以及 W3 的取消/外部回调，仍是完整语言任务；`validation/product-gates.json` 的 Rust 成本/演进条件仍是产品门。这里不把 B0、181、W2、W3、完整 1.4 或产品门改为完成。
- 仓库没有一张已冻结、能将“所有 ZUI 开工前”直接归约为“所有 1.4 功能”的精确验收清单。本矩阵按既有消费者解释同步基础范围；不替用户删除声明式、异构扩展或外部生命周期需求。

## 可复核身份

新探针使用独立构建 CLI，源提交 `f581cd67d1f36b088111a4afc55d308a0a47fe59`（206 语言 + 212 编译集成候选），CLI SHA-256 `efc1ee72a25722ec10262ade600242b1a1e27fb09aba5fe733d07ab30baaf037`。构建收据：`../review213/compiler-build.json`。它尚未含 214–216，因此本轮已有表达能力不是借用了待验 Product 捕获。

`SOURCE-MANIFEST.json` 绑定规范、生产定位、既有消费者、历史收据及全部新探针的逐文件 SHA-256。`RESULT.json` 保存首批 25 次原始 check；`FOLLOWUP.json` 保存 4 次补充 check；失败原件没有改标。`indexed-callback-delete/NATIVE.json` 保存两次真实构建与执行。所有新源位于本目录相应 `*/main.zil`。

旧 203–206 的 Source/raw/Wire/native/故障结果只引用各自固定收据，**不算本轮重跑**。214 是本审计者所写，不由本报告重复自我独审；其状态只列“作者交付，组合审核待完成”。

## 闭合矩阵

| 消费需求 | 已有证据与本轮观察 | 当前判定 | 仍需行动 |
|---|---|---|---|
| 动态字节、事件队列、UTF-8/UTF-16 准备、文本编辑 | `examples/mainline/zui_boundary159/{main,queue,state}.zil`；`owned_text/edit.zil`、`text_decode/text.zil` 是普通语言算法。相关真实执行属于既有 139/159/124 收据，非本轮新成绩。 | 同步基础可复用。完整 String 不变量、grapheme/IME/排版不等于全部要由编译器实现。 | 在同步消费者中复用这些真实 API；字体、排版、IME 属对应库/平台范围，不凭标题发语言单。 |
| 普通静态控件协议与公共泛型函数 | `impl147_static_traits.rs` 有源→Verify→O0/O2 的 512 矩形布局消费者；`impl162_trait_bounds.rs` 已覆盖定义期有限名义 bound。 | 不是“traits 尚未实现”。当前静态协议可作为框架基础。 | 关联类型、默认/泛型 trait 方法、life/HRTB 是更大抽象范围，不能从当前有限能力外推。 |
| 状态保存多个真实回调、同型不同实例与模式 | `state_bindings203/{state,factory,main}.zil` 保存 F/G，不同模式独立；203 最终收据有 raw/Wire 与两平台各 128 次执行/88 次真实分配失败。原旧双 callback 探针本轮已 Verify 成功。 | 已闭合有限同步能力。 | 不再派同名旧缺口；开放异构运行时集合仍另列。 |
| 普通完整 T 的容器库、状态更新与替换 | 202 普通 StoreElement 模板，203 `store_helpers.zil`，205 源与正式收据；205 参数顺序、别名代次、失败准备均有实际控制。 | 已闭合目前完整 T / State profile。 | 组合时保持真实来源和清理；无需再次做 202–205。 |
| Store 和多层状态留在回调环境 | 204 有 Store 拥有捕获与 State/外层 callback Store 的五模块、双核、native/fault 证据。206 有普通 nested Product。214 的 20 组新真实源包含 Product factory、State、outer Store。 | Store 捕获已闭合；Product 捕获正在 214–216 纵向收尾，不能提前记全部通过。 | **必须完成**独立 Analyze/Verify、raw/Wire、O0/O2、失败清理与集成回归。只读默认 Product 借入仍未实施，不得用含糊规范句计通过。 |
| 动态节点、关系、独立删除和已有回调继续运行 | 本轮 `indexed-callback-delete/main.zil`：普通 Node<F> +泛型 `seed/erase/release<T:StoreElement>`，删除中间 ID 2；剩余 ID 1/3 和 parent ID 保持；FnMut 继续得到 12/32。源 SHA-256 `48b9761e1c00dc0d16a365e3e8a7c6f20b018bf3a3c3b0033bca93925a0e19a4`。O0/O2 都输出 `B!AC`、exit0。 | 已证明索引/ID + 同质存储是实际可表达路线。嵌套 Store 和拥有 Sum **不是该操作不可替代前置**。 | 尚缺更完整的同步跨模块组合：重挂接、ID 失效处理、事件调度/更新/删除交错、故障路径。当前 erase 为 O(n) suffix 转移，不声称效率达标。 |
| 当前调用借入、显式 &/&mut 和 slice 参数 | 184 lending、189 地址、193 slice 参数有源/raw/Wire/native专项；原 closure address/mut address 与 nested/loop helper 源本轮均 Verify 成功。 | 可通过显式参数传入调用期视图；不必先实现所有捕获语法。 | 直接 slice capture 仍 Types Unsupported，短期可传现有 slice 参数。需要保存借入视图或通用 lending 时才构成对应真正缺口。 |
| 稳定的同步平台事件和绘制入口 | `win32/platform.zil` 的静态无环境 WndProc、消息循环、资源包装与按地址 C record 已有真实 Wine 窗口证据；未用 C UI 宿主。 | 已建立有限平台起点。不是 retained ZIL userdata、完整 GUI 或原生 Windows 验收。 | 平台层、绘制/布局/可访问性由 ZUI 实现；本轮平台未重跑，原生 Windows 保持 NotRun。 |
| Product 本身进入 Store、含 Store 的 Product、嵌套 Store 元素、Product 错误载荷 | 本轮 `product-store-element`、`product-owning-store`、`nested-store-element`、`product-error-union` 均在 Types 明确 Unsupported。生产门为 `packages/product.rs`、`dynamic_store.rs`、`aggregates.rs`。 | **真实语言缺口，但不是已经证明不可规避的同步开工阻塞。** 扁平 State、捕获环境、StoreElement 完整 T 与 ID 关系已有替代路线。 | 保留清单。只有真实公共 API 无法按这些路线实现时，再开有限形状任务；不一次放开所有递归组合。 |
| 可选状态和拥有 Sum | 普通 `Option<T>` Copy enum 本轮 `copy-option-library-control` 已 Verify；`i32?` 表面型与 `enum Maybe{Empty,Some(Leaf)}` 仍 Unsupported。 | “Option 全部缺失”不成立。拥有 variant/任意状态资源切换是真缺口；Copy tag/ID 或原错误联合可覆盖不少基础情形。 | 若要求一个通用值直接持有任意活动控件/资源 variant，需真正 Sum 布局、tag、来源与清理；否则不为基础索引状态强制增加。 |
| 任意用户 callback 的统一异构集合、开放式组件插件 | 当前 Store 可持有限具体 F/G 状态；`dyn-valid-parameter` 明确 Types Unsupported，所有环境不能仅因同布局合并。 | 对“运行时任意类型统一注册并保存”的 API 是不可替代的语言问题；对已知同质 F/G 家族 + ID/表驱动则不是。 | 若把开放异构接口列为开工要求，最小方案需固定 erased/dyn 合同或真实拥有 Sum，二者择所需语义，不能靠不安全全局指针表/隐式盒子绕过。尚无仓库源消费者证明该开放范围是同步起步硬门。 |
| 任意泛型 F 原样返回、拥有参数/结果的 callback | `callback-return-generic` 在 Expand 拒绝；`closure-package-param` 在 Types 拒绝。已有 State<T,F> 持有、完整 T identity、直接字段调用可避免常见简单转发。 | 真实一般高阶能力缺口；未证明所有框架 API 必须以这种签名表达。 | 用所需 public API 的最小源确定边界，不能只因 `fn keep<F>(f:F)->F` 不接受就阻止所有保存/调用方案。 |
| 声明式 `ui!`、Type/Syntax、生成类型/impl、卫生与映射 | 合法 `syntax-valid-alias` 本轮 Expand Unsupported；`ct-item-if` / `ct-statement-if` 明确 Unsupported；CT 常量控制成功。静态 Traits 的关联项源也明确拒绝。 | **声明式表面层的真缺口**。只是标量 CT 或 parser 识别不能算完成；亦不是普通同步源码统一开工门。 | 在开始声明式 ZUI API 前，需要受预算生成与封闭阶段、卫生绑定、映射/预览/格式化、生成/手写同核及等价消费者。不是仅放开 #if 就完成。 |
| 外部长期 retained userdata、重入 Active handler、取消/挂起/并发 | §9.5、§11、§12 的完整合同仍未闭合；生产 FFI 明定 noescape/static 无环境范围。 | 对相应系统是硬缺口；当前同步、ZIL 自己驱动的事件循环可先不跨这条边界。 | 需要时先稳定位置/注册失败/停止接收/注销排空/no-future/最终执行者清理；异步 SABI 和线程同步另验。不能借静态回调成功宣布完成。 |

## 最小仍需施工事项

### 1. 完成已派 214–216：这是当前确定的语言施工

保持完整 Product profile、独立责任槽和来源；完成未调用清理、Fn/FnMut/FnOnce 普通/错误出口、State/outer Store 的 source→双核→机器闭环。不要用 214 Types 通过替代 215/216，也不把作者报告当独审。

### 2. 冻结一个有实际规模和故障的同步语言消费者：这是必须的组合验收

建议下一编号首先承接消费者，不预先承诺扩语义。至少拆为 model/store、callback、event、update、driver 五个普通源码模块，组合：

1. 运行时数量的节点；父子/同层关系用明确 ID/索引，既有资源与 F/G 回调保留实际值身份；事件数据来自实际输入。
2. 创建、同型不同实例、重复 dispatch、字段替换、节点新增/独立删除/重挂接；资源不能靠 tombstone 一直保留到程序结束来冒充独立关闭。
3. 214 后的完整 Product 环境和204的内捕 Store 分别经过 State/外层 Store；同字段活动窗口反例、陈旧实例/ID的库级检查、无关兄弟合法操作均可观测。
4. 逐真实申请失败、替换 RHS 失败、分配前后退出、清理先后和最终 live0；同时保留0/1/多轮、continue/break、早返/错误控制。
5. Source、独立 raw/Wire 最近反例、O0/O2 和已声明目标分别有身份；此处不启动 GUI 开发，也不引入新的产品阈值。

通过后可以精确报告“同步基础消费者所需语言闭合”，仍不能把通用声明式/开放异构/长期外部回调/完整1.4一并改成完成。若消费者发现真实不可表达操作，必须保存原源并按具体操作发新的语言片；当前证据没有支持再指定一个不可替代的 Store 嵌套或拥有 Sum 必做单。

### 3. 声明式与开放异构按所需 API 分别派工，不冒充已完成，也不阻塞已有同步库能力

已有规范要求这些终态能力，不能从目标删去。下一优先级应由实际 ZUI public API 确定：显式 UI 扩展对应 §16/§18.2；动态任意 handler 集合对应 §9.5/§18.1；长期 OS 环境对应 §12.4。三者各有真实未支持源/生产门，但不是同一个协议，也不是开几个前门就能同时完成。

## 证据限制与失败原件

- 首批旧 `dyn-parameter` 实际缺 trait 方法分号，先 Parse 拒绝；补充 `dyn-valid-parameter` 才是用于能力判断的合法形状 Unsupported。首批 `syntax-extension` 的保留字路径也先 Parse 拒绝；能力判断使用 `syntax-valid-alias` 的明确 Expand Unsupported。
- `indexed-callback-delete` 首稿使用了当前 parser 不接受的裸局部块；原源/结果保存在 `attempt01-source.zil`、`attempt01-result.json`。最终用普通泛型 `release<T:StoreElement>` 明确消费选中节点，经 Verify 和机器执行。没有改编译器或接受性以通过探针。
- 旧文件名 `closure-owned-error-result` 的实际源码返回直接资源，并非 ErrorUnion；不能以这个文件名推断所有拥有错误结果仍未实现。
- 新删除控制没有实际分配失败注入，也没有对多模块 API、长期图/循环图、任意异构节点或平台回调做完整验收。`B!AC` 证明此输入的实际清理和继续调用，不构成完整性能或产品结论。
- 生产文件未改动；结果应由集成方挑选并提交到正式审计位置后才作为交付引用，本 recovery 文档本身不变更工单状态。
