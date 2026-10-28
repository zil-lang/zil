# 170 起语言主线：三条有限施工建议

本文是接续建议，不是派工、实现承诺或验收结果；不改变任务状态与源码。
依据：169 主树 b6b36af、当前规范摘要 9eeacdb5333925a084d7dc14a0c4b50b1a3d01684acb8214ea4bd381251ed666。
4668c67 的 helper/factory 顺序修复已独审通过，最终整仓1463项通过；实际开工应绑定最后交付的源码身份。
优先级：B 可最快形成语言增量，C 可独立关闭原资源头地址探针，A 先固定公共存储合同再贯通原生路径。
三线同时施工，但共同类型/IR/Wire/规范摘要仍由 root 单写，阶段证据与最终组合分开。

## 已有基础与不能重复计功的能力

schema19 已有完整 Package 错误结果、固定 Package/Closure 数组、具体闭包转发/再捕获与私有替换槽。[S2]
错误联合可承载完整 Package，但不推断直接资源的 Managed 返回模式；其整局部重赋值仍关闭。[S3]
数组操作 72–77/79 使用固定长度、静态下标和实际局部壳；它们不是动态容器协议。[S4]
BorrowCapture 78 和递归闭包清理可复用；不同 opaque 工厂不能因为机器 ClosureId 相同而混赋。[S5]
资源默认 self 为 Desc，give self 显式消费已实现；原 &Self 头地址探针仍需真正的 Address。[S1,S6]
ByteBuffer 的 reserve/失败保持/分配观测可复用为设计和测试依据；字节内容不能重解释成任意拥有元素。[S7]

| 建议线 | 首个可封口结果 | 与另外两线的依赖 |
|---|---|---|
| A 一般拥有元素动态存储 | 同质完整 Package 的运行时初始化前缀、显式扩容、尾部取出与逐项清理 | 首片不依赖嵌套 Package 字段或资源头地址 |
| B Package 内拥有字段 | 完整固定数组字段的构造、整体取出/补回/替换与递归销毁 | 复用 167；先不要求动态容器或泛型闭包字段 |
| C 真实资源头地址 | 有限资源实际管理槽的共享 &Self 方法与跨模块调用 | 复用现有地址/资源检查，独立于 A/B |

## A：动态存储先做完整前缀，后做任意孔洞

**首片范围。** 元素先限当前完整 Package，容器保持 `[0,len)` 全部初始化、`len..cap` 无责任。
先完成空容器、显式 reserve、追加、尾部取出、长度读取、完整移动和销毁；暂不做任意索引移出。
动态数量使用运行时循环与前缀事实；不能把未知多个元素标成静态 One，也不能按 cap 枚举编译器槽位。[S1,S4]
reserve 先检查容量/布局乘积与对齐，分配失败保持原头、元素和责任；成功后一次转交，旧块不再清理元素。
追加先完成元素准备，再写入并提交长度；尾部取出先撤销相应前缀责任，再转交完整元素。
建议追加失败通过 `Package!Unit` 返回未插入元素，复用 166；不能让 bool 失败同时暗示调用者仍拥有已传参的值。
此建议是公共接口待定项，不能在入口尚未登记时伪造 `vec_new<T>` 等已存在裸原语。

**最小源码消费者。** 四模块 `item/store/work/main`，Item 沿用 Ticket+ByteBuffer 的真实 Drop 模型。
main 从已有 getchar 获取运行时数量；普通函数循环构造 Item，经已定的 store API 追加，再逐个尾取并累加 code。
独立入口令第一次扩容、已有两个元素后的扩容、后续 Item 准备分别失败，核对实际剩余责任。
另一个入口将完整容器跨模块转交后销毁，检查每个 Ticket 一次清理和 payload 分配/释放平衡。
这些函数名是普通库入口的职责描述；源码签名必须等容器的 Value/Managed 类别及失败合同确定后才冻结。
没有前述公开合同之前，提交接口提案与现有约束探针，不把手写 C 容器或手填 IR 计作源码完成。

**最近反例。** 读取未初始化尾部、把 len 改大后发布、同一 Package SSA 插入两次、尾取后再次取旧槽。
扩容/移动/销毁后使用旧窗口；失败 reserve 冒充新分配；不同名字的同一容器冒充不交。
容量乘 stride 溢出、畸形负长度、缺失元素清理、释放 payload 后再访问其中的责任记录。[S1,S7]
若准入零尺寸元素，元素计数与责任不能靠指针是否相等判定；否则首片明确拒绝该形状。

**公共 IR 影响。** 需要明确的元素类型/布局和动态头语义；现有 ByteBuffer 类型与操作不能借壳授信。
追加、尾取、扩容、清理必须携带真实容器来源和前后状态；不能把 SSA index 塞进现有 u32 静态槽字段。
可能新增操作或载体类型，编号/版本由 root 在最小提案后统一分配；既有 72–79 的编码不得改义。
类型与 wire 元数据先计费再展开；Verify 以有限前缀/唯一载体事实证明，运行时长度不制造无限静态枚举。
LLVM 可复用显式 allocator ABI 与既有元素清理函数，但搬迁不得偷偷复制闭包旗标或重复 Managed 头。

**实现入口。** 新动态存储模块与独立 Verify 配套；复用各阶段自己的资源/来源分析，禁止共享前端证明。[S4,S7,S8]
首片不承诺外借元素地址、异构回调、自动增长策略、并发或稳定地址。
闭包元素作为下一增量：复用 168 的实际环境布局/清理，但另核 opaque 身份与容器来源。

## B：先把完整拥有数组作为 Package 字段

**首片范围。** 现有 `package` 与普通字段语法即可描述 `[Item;2]`；不增加 `manages` 的第二种拼写。
`manages tag: Ticket` 仍是 Managed 根；`items: [Item;2]` 是按值拥有字段，不能降成 Copy 或 Desc。[S1,S9]
先允许整个数组字段取出、补回和替换；在数组成为普通局部后使用已有静态槽操作。
这样先保持父字段的“完整/缺失”两种形状，不同时承诺 `state.items[0]` 在父壳内的嵌套部分状态。
父壳缺少数组字段时只能清理剩余字段，完整移动/返回仍要求该字段完整补回。
字段替换先准备 RHS；失败保持旧字段，成功后清旧字段再放新值，父壳清理位置不重新出生。

**最小源码消费者（目标源码；当前拒绝边界应保留作前置证据）。**
```zil
package State { manages tag: Ticket; items: [Item; 2] }
fn relay(s: State) -> State { return s }
fn exercise() -> i32 {
    var state = relay(State { tag: Ticket { code: 90 }, items: [make(65), make(66)] })
    var items = state.items
    let old = items[0]
    items[0] = make(67)
    state.items = items
    return old.code - 65
}
```
Ticket、Item、make 使用已有普通声明与构造；两模块补公开导出，Drop 日志区分 old、元素与父 tag。
再用当前 `E!Item` 构造器制造新数组后续元素失败；另测先取字段再早退，以及补回后整个 State 返回。
若实际源码解析存在既有字段消费分歧，只调整语义实现，不发明局部 move 关键字或新拆包语法。

**最近反例。** Load/ExtractField 复制拥有数组、字段已取走再读、缺字段整体返回、同一 RHS 两次补回。
未来仍使用的元素 Desc/闭包地址跨父字段替换；子数组缺元素时冒充完整父字段；重复或漏掉子清理。
父 Package 被捕获时不能因 `supported()` 变宽而绕过尚未扩展的环境视图协议：未实现处显式关闭。[S9,S10]

**公共 IR 影响。** Type::Package 已可编码字段类型，物理编码未必新增；消费/清理语义必须显式更新。
现有 Take/Drop/StorePackageField 只处理直接 Managed；若扩展到非Copy Value，必须按精确字段类型核验。[S9]
MakePackage 消费全部拥有输入；父字段旗标与子布局清理分开，移动后不得保留旧父壳来源。
无需先发明通用 Place 路径：首片可用父字段整取→完整数组局部→整字段补回，避免同时扩嵌套借用。
独立 Verify、Lower/LLVM、接口指纹与 work/depth 预算都必须同步；嵌套展开按实际乘积收费。[S8,S10]

**后续回调字段。** 不把 `impl Fn` 放到当前不允许的字段位置，也不把有环境值擦成 `fn()`。
先核对既有泛型 Package 实例化能否保留 source opaque 与具体 ClosureId；文法允许泛型不代表此链已完成。[S5,S11]
到该前置成立后，增加自包含具体回调字段的整取/借入调用/替换/递归清理；不同时开放外借环境或 dyn。

## C：从原 &Self 探针建立真实资源头窗口

**首片范围。** 先有限 Copy payload 的用户资源、实际 Managed 局部的共享头地址、同步不逃逸方法。
必须保留原探针，不能把 `&Self` 改成默认 Desc 方法再宣称已关闭。[S1,S6]
```zil
trait Read { fn read(self: &Self) -> i32; }
resource C { x: i32 }
impl Read for C { fn read() -> i32 => .x }
fn main() -> i32 { let c = C { x: 7 }; return c.read() - 7 }
```
先保留原空 main 声明探针，再跑上述真实调用；加跨模块 trait/资源/实现与普通 `&C` 参数调用。
头地址必须指向同一实际内联载体，不能创建第二个 Managed 头或把显式地址重写为 Desc。[S1]

**最近反例。** 活头地址之后 give/替换/drop 原载体；返回局部头地址；把 Desc 指针重标成合法头窗口。
同一头共享窗口下写字段、共享升级独占、把整个 Managed 值经 ReadAddress 复制或经 WriteAddress 覆盖。
同布局异名资源混用、隐藏 Drop 作为普通方法调用、被移动旧槽重新初始化后复活旧地址。
首片只有实际 Managed 槽能建立窗口；Desc 到头地址必须另有可核来源和存活证明，不能靠指针转换开放。

**公共 IR 影响。** 先定 `Address<Managed<R>>` 或等价名义头视图的内部形状；地址本身绝不取得清理责任。
可能复用 BorrowLocal/Reborrow/AddressField 的字段编码，但现有准入、来源与 loan/resource 交叉检查必须扩展。[S6,S12]
整体 ReadAddress/WriteAddress 保持禁止，只开放已经核定的 Copy payload 投影；give 与窗口冲突独立重建。
LLVM 使用实际局部头指针，不附加未经证明的 noalias，不复制 pointer/len/cap，不自动装箱固定地址。
`&mut Self`、ByteBuffer 密封头、字段内头地址与返回来源在共享首片通过后分别增量；不一次取消全部限制。

## 合并与验收顺序

B/C 的有限首片可先实现；A 在容器公开合同确定后继续，不等 B/C 的更大扩展。
共同拥有类型分类、字段/环境布局与效果摘要的接口请求先合并；三线各自重建自己的安全证明。
每线保留源码正例/最近拒绝、raw 与 wire 篡改、O0/O2 清理，以及 N/N−1 元数据预算反例。
root 最终消费者组合 A 的容器、B 的完整 State 转交、C 的同步头读取；只组合已通过各自前置的形状。
限定 Linux x64 原生证据；Win64、全语言、产品条件与 Rust 成本比较不从“内联+无泄漏”推导。

## 源码与规范依据

[S1] `_zil_design.md`：§2.1–2.4、§3.1、§4、§7、§8、§9.3–9.6；明确类别、One/Many、窗口、准备和清理。
[S2] `spec/interfaces.json`：`implemented_partial_schema` 的 `owned_error_union19`、`owned_array19`、`retained_closure19`。
[S3] `crates/zil-types/src/errors.rs`；`lib.rs::stmt_inner` 的 owned-union reassignment guard；`zil-verify/src/errors.rs`。
[S4] `zil-types/src/packages.rs::owned_array_place`；`zil-verify/src/resources/arrays.rs`；`zil-backend-llvm/src/emit/arrays.rs`。
[S5] `zil-types/src/inference.rs` 的 factory opaque/ClosureArray；`retained.rs`；`closures/slots.rs`；`zil-verify/src/retained.rs`。
[S6] `zil-types/src/aggregates.rs` 的 Address 解析和 nominal method call；`zil-types/src/addresses.rs` 的借入/实参路径。
[S7] `zil-types/src/byte_buffer.rs`；`zil-verify/src/byte_buffers.rs` 与 `borrows/buffer_effects.rs`；LLVM `emit/byte_buffer.rs`。
[S8] `zil-verify/src/closures.rs::check_layouts/type_layout_work`；`zil-types/src/packages.rs::check_owned_array_closure`。
[S9] `zil-types/src/aggregates.rs::Resolver::ty` 的字段准入；`packages.rs::supported`；Analyze/Verify 的 `resources.rs`。
[S10] `zil-verify/src/closures.rs` owned-array/owned-union capture guard；LLVM `emit/packages.rs` 与 `resource_closures.rs`。
[S11] `spec/grammar.ebnf` 的 Package/Generics；`zil-types/src/aggregates.rs` 非Copy字段拒绝；文法与实现须分别核对。
[S12] `zil-verify/src/lib.rs::check_function_signature_type/check_local_type`；`borrows/roots.rs`、`borrows/loans.rs`。
