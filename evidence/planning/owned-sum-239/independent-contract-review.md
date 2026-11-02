# Q 上有限普通拥有 Sum：非作者只读设计审查

审查基线：`/workspace/zil-core` HEAD `e92508be996a190fa188d08319d37cc8b522bae6`（Q）。作者方案：`/tmp/owned-sum-design-Q.md`。本文是待冻结合同建议，不是实现验收；没有修改 core、主树规范、生产源码或公共 schema，没有执行新功能测试。

root 最新范围优先：本片只扩展穷尽 match，不新增 guard 语法。实查 Syntax/Core MatchArm 只有 pattern/body/span；作者方案第 9 节关于未来 guard 的开发要求不属于本片。现有 if-let/while-let Copy 门不应因共用 pattern helper 被顺带打开。

本稿已按 root 最后指示收敛：递归 owned Variant 模式完整支持；本地观察带 typed path；Unpack 直接 local；保留实际成功初始化年龄。曾讨论的“Unpack 子责任继承父年龄”已撤回，不是本稿建议或实现权限。

## 结论与需先冻结的选择

建议采用隐藏完整 Enum local 保管 scrutinee、显式本地观察、`UnpackVariant { local, variant, fields }` 原子消费，以及 `DropEnum { local }`。不建议把 `Load(owned Enum)`解释成同 Type 的特殊非拥有 view。公共 IR 数量由完整权限协议决定，不能只取消 Load/Read 的拒绝来凑两个 opcode。

| 选择 | 最小完整合同 | 未冻结时的风险 |
| --- | --- | --- |
| 观察与消费源 | TagLocal / Copy ReadLocal 只观察同一完整 live local；Unpack 直接以该 local 为源 | SSA Take 与旧 local tag 证据失配；同 Type 的 view 被当 owner 转交 |
| 递归 pattern 范围 | 完整递归 owned Variant 模式；两条 local 观察使用有界 typed projection path；所有模式测试成功前不拆 | 外层提前拆开后内层失败，下一 arm 已无完整 scrutinee |
| 同时输出次序 | Unpack 目的 locals 按 fields 逆序真实登记；递归拆解到 temps 后，最终绑定/隐藏叶按整个模式声明序的逆序 Take→Store 发布 | 仅每层反序不足，右侧嵌套字段会跑到左侧字段前清理；私有 rank 违反现有年龄规则 |
| 整值 Sum 赋值 | 复用私有稳定 Product shell 可以；必须区分 shell 存活与源 Sum 已被普通移动 | Store 重新排年龄，或空壳让已移动名字复活 |
| 来源模型 | 按完整结构、variant、payload index、真实资源根保存；Make/Unpack 只转交，不创造来源 | Incoming 洗成 Created；全部降 Incoming 误拒真实 fresh 正例；同型 Store 根归并 |
| 嵌套析构 | Product / Enum / ErrorUnion 使用实际 tag 的结构 DropPlan | 静态 leaf 列表错误清理 inactive branch、漏嵌套 Store |

## 1. 本片模式的最小完整协议

当前 CorePattern 已有递归 Variant / Tuple、整数/Bool、Name/_，没有 Product 字段模式。四条指令的建议形状为：

```
VariantProjection { variant: u32, index: u32 }
VariantTagLocal { local, path: Vec<VariantProjection> } -> i32
ReadVariantLocal { local, path: Vec<VariantProjection> } -> Copy value
UnpackVariant { local, variant, fields: Vec<LocalId> } // 无 result
DropEnum { local }                                  // 无 result
```

所有 match 先把完整 scrutinee 转入真实隐藏 Enum local。模式判定期间它保有完整责任；反驳边继续用同一个完整值。成功选臂后才消费。Whole Name 模式转交完整 Enum；whole `_` 模式保留一个真实隐藏完整 local，按明确的 arm 生命周期自动清理，不能当作丢弃责任。

路径从 root local 的完整 Type 开始，每个 `(variant,index)` 在当前实际 Enum 上选择该 variant 的 payload field。每层都核索引、完整名义 Type 和当前 tag；字段成为下一层 Type。TagLocal 的空 path 合法，末端必须是 Enum；ReadLocal path 必须非空，末端必须是现有 finite Copy Type。终端 Copy tuple/Copy enum 可以交给已有 Copy SSA pattern 逻辑，绝不产出 owner、Address、Desc 或通用 view。

路径中每个下一步的源须仍是 Enum，而不是按布局把 Product/tuple/ErrorUnion 当 Enum 解释。现有 CorePattern 没有 Product 字段模式，也没有 ErrorUnion variant 模式；ErrorUnion 载荷仍可绑定整体后沿旧 `?`/error 协议消费。若公共 IR 有意同时允许 ErrorUnion 路径，则必须另定 0/1 分支及 tag 类型，不能从“Sum”称谓偷偷推导。本文推荐路径只用真实 Type::Enum，已覆盖当前递归 Variant 模式。

因此 `Outer.Wrap(Inner.Some(p))` 的 Inner 即使 owned，也可沿原完整 Outer local 观察其 tag 和 Copy 叶，模式失败边保持 Outer。所有纯模式测试成功后才逐层 Unpack；不能先拆外层再让内层 pattern 的失败边直接去下一 arm。

## 2. tag 证据与当前激活

TagLocal 的 i32 是观察结果，不能单独授权后来的 payload。ReadLocal 与 Unpack 要求所选 variant 对同一实际 local 当前完整值成立：构造已知 tag，或规范 CFG 的准确等值成功 edge。相反 edge、仅同 Type、不同 local、额外绕过前驱、旧标量 tag 都不成立。

最短 producer 可在选定模式后重读 tag，建立唯一成功块，在其中发目标 StorageLive 和 Unpack；无需实现通用支配证明。Copy tests 位于首次 tag 测试和最后 Unpack 之间时，不能误把旧 ErrorUnion 的“直接前驱”实现当作已可接受；应生成最终重查，或明确核验一个有限纯 corridor。嵌套投影每一层也需同样的父 tag 事实。

path 事实可以表述为 `(真实root local当前激活, prefix, selected variant)`：根 tag 证明授权第一步投影，下一层 tag 证明授权下一步。Read 要求其每个前缀的所选 tag 都成立；同型兄弟 path 的事实不能混用。join 只保留各前驱共同有效事实。若 TagLocal 后 source 被改、随后才 branch 比较旧 i32，不能在该 edge 重新制造事实；观察 SSA 自身的有效性也须被 source 写入杀死。纯 pattern corridor 中允许常量/Copy 计算/branch，不允许调用、注册 defer、source 修改或其它用户 effect 借机跨过观察权限。

选臂提交中，父 Unpack 已把 root 变空；对子 Enum 不直接复用已失效的 root-path 事实。可以在新 child local 重新 TagLocal→准确成功 edge→Unpack。错 tag edge 用显式 Trap 防止后端 UB；runtime Trap 不是静态成功 edge 证明的替代品，也不是可恢复事务失败。

任何 Take / Store / Drop / StorageDead / 新 StorageLive 激活都杀死旧 local 的观察事实；即使新值同 variant 也不能恢复旧 payload 来源。只记录 `(LocalId, 静态Store指令编号)` 不足：循环中同一个 Store 点可执行多次。必须是沿 CFG 被写入杀死、在 join 取共同有效事实的证明，或每次 consume 前现取的短 corridor。不得信 producer 写的私有版本号。

新 op 的类型规则和资源规则都必须拒绝 moved、dead、半初始化来源。旧 SSA VariantTag/ReadVariant 保留 Copy 路径；其 ReadVariant 对 owned payload 不能产生 owner，Load 也不能物理复制完整 Enum 责任。观察 op 不能跳过已有 active loans/Store 窗口权限核。

## 3. 原子 Unpack 与 scope/清理年龄

先检查并预付全部工作，再消费源或初始化任何目标：完整 nominal Type、存在 variant、实际 tag、精确 arity、每个字段顺序和完整 Type、所有 LocalId 边界、去重、live storage、vacant 状态，以及源与目标互异。Copy/Unit 输出也要真实初始化，不能覆盖一个已有 Copy local 使 definite assignment 状态含糊。`[]` 只对应零字段 variant，仍消费完整线性 carrier。

建议输出全属于实际 arm scope。外层结果槽通过后续普通转交填入，不能让 atomic output 隐式跨 scope 发表。参数/capture view/别的激活不能当 vacant 目标。所有初始化、scope、loan、defer 和后端 flag 使用者都要识别它的多 local 定义；不能只改主 verifier 的 opcode type match。

Unpack 的同时初始化次序定义为 fields 逆序，Analyze/Verify 独立重建；保持现有实际成功初始化规则，不继承源 carrier 的年龄。真实输出 Vec 决定字段映射，LocalId 预留顺序不决定年龄。

递归 pattern 不能只在每层逆序登记。例如 `Outer.Pair(a:R, b:Inner.Pair(c:R,d:R))`：外层登记 b→a，再拆 b 登记 d→c，直接退出会是 c,d,a。源面要求 a,c,d 时，producer 在所有模式测试成功后进入无用户调用、无可恢复失败、无早退的 commit corridor，逐层拆入真实中间 temps；最后将所有最终 owning Name/_ 叶按全 pattern 递归声明序的逆序逐个 Take(temp)→Store(final binding)。未继续拆的 whole Product/Sum 绑定视为一叶。该例最终登记 d→c→a，退出为 a,c,d；所有中间 owning temps 真实且在 corridor 末尾清空。

核只验证这些实际操作、实际登记和实际 cleanup 顺序，不信 producer 的“这是pattern”标记或私有 rank。不需新增 raw Unpack 特殊年龄权限。Copy/Unit 仍完整发表；两 owning 字段、同型字段、Copy/Unit 插在中间、左右两侧嵌套 owned Sum、部分 whole 子树绑定都需覆盖。

每个 `_` owning payload 都有隐藏真实 local。选中 arm 的 return、`?`、break、continue、正常尾、defer 都沿既有 scope 清理。移动出某字段只移走其实际责任，余字段照序清理。body 内 defer 在实际注册点参与混合次序；不能把 Unpack 当免责批量动作。外层 hidden scrutinee 被 Unpack 后已空，不得在 arm 退出再次 DropEnum。

## 4. 来源必须随真实映射转移

建议结构路径至少区分 ProductField、SumVariant、PayloadIndex 及后续层级，绑定实际 carrier/leaf 身份。仅最后字段号、Type 相等、同名资源或统一 Incoming 都不够。

MakeVariant 是封装，Unpack 是转交；都不创建叶资源。完整 Sum 进出 Product 字段、参数、返回、ErrorUnion、再拆回时，保持真实 Created/Incoming、Store 根及其它已有描述/窗口事实。Unknown tag carrier 以各 variant 的可行来源保存；inactive branch 不制造同时存在的债。join 保守合并实际可能来源，不能因某 branch fresh 就把共同结果标 fresh。多个独立 Sum 使用每 carrier 的 tag/source 集合，不枚举全局 tag 笛卡尔积。

一个必须保留的正反对：同一 variant 有 `incoming R` 与 `freshly-created R` 两个同型字段，外包 Product 后再拆。返回实际 fresh 字段的 create 合同应能通过，返回实际 incoming 字段冒称 create 应拒绝。若仅统一 Incoming，负例拒了但正例误拒；若统一 Created，出现责任洗白。

需要修正作者草案“交换同型 payload 一律拒”的表述：`fields: [dest_b,dest_a]` 本身可以是安全、有意的真实字段映射，核不能按变量名字拒绝。应允许无错误合同的实际置换，并跟随它携带来源；攻击是置换后错误宣称 Created/旧 Store 根，而非置换本身。

ErrorUnion 两侧的新完整形状也保持同一协议：只选实际 tag，`?` 先真实转交 error，再准备 outgoing carrier 后清当前 scope；已移动实参没有 rollback。不能因新 Sum 扩展抹掉现有 ErrorUnion 的显式注入/歧义规则。

## 5. 稳定 slot、构造失败和后端闭环

稳定私有 `Package { value: Sum }` 可复用已有 shell/字段协议；要冻结它只是表示映射，不改变公开 Enum ABI，也不授予通用借入。普通 whole move 后 source binding 应保持已移动，不能仅因私有 shell 仍分配着就接受 Copy 偷读或后续复活；显式允许的稳定赋值则遵守其独立 vacant/publish 状态。

MakeVariant 必须按 source 顺序准备每个 payload，成功拥有前缀立即有真实临时责任。第 2/3 字段失败时只逆成功准备序清前缀。完整 RHS 先准备，再清旧 variant，最后无失败发表。独立 RHS 失败保持旧活动值；self 实参已转交后失败只清真实 prepared 值，不捏造恢复。

Product<Sum> 不能只放行类型：其 take/refill、238 完整替换、DropPackage/DropPackageField、ErrorUnion 包装和 native error 入口都须走实际 tag 的递归析构。静态 `teardown_paths` 不足。私有 DropPlan 可共享后端 emission，但不成为前端或双核权限证明。析构先清责任 flag，再按真实 tag 和字段声明序递归；非法 tag 显式 trap，不能使用 LLVM UB 隐去分支。保留 Enum / ErrorUnion 原 ABI、对齐、padding、Bool、Unit 和溢出边界。

形状闭包必须仍有限无环：Copy | 已核 Managed 叶 | Product | Sum。普通准入不能反向放开 capture、StoreElement、owning array、borrow/shared、resource header、generic Enum、FFI。裸 R 保持 Desc；只有显式 manages R 才表示 Managed，不能从类型位置猜出权限。旧 Copy Enum 和旧 ErrorUnion 已有 profile 不得重关或增费。

## 6. 独立攻击与实证门（尚未执行）

| 组 | 最近 raw/Wire 反例 | 对应 source/native 正例 |
| --- | --- | --- |
| tag | 错构造 tag、相反 edge、额外前驱、同 Type 另源、旧 tag 跨 Store/loop/StorageLive | Copy literal 失败再下一 arm；最后 variant 明确证明 |
| 原子转交 | duplicate Unpack、零字段再次消费、duplicate/越界/错 Type/非 vacant 目标、跨 scope 目标、源目标重叠 | 两个以上 owner、Unit/全Copy活动 branch、完整 move/参数/返回 |
| 观察权限 | Load owning Enum、Read owner、Tag/Read moved 或半初始化源、nested path 缺父 tag | Copy tuple/Copy Enum 深模式；若准入则 nested owned variant 模式 |
| 清理 | 漏 `_` 目标、只发表一半、错误初始化年龄、body defer/reorder、Unpack 后再 Drop 源 | Name/_ whole/payload；return/?/break/continue；真实 declaration-order oracle |
| 来源 | Incoming 假 create、两同型 Store 根串换、包装后全清 Incoming、phi/loop 洗白 | fresh 字段 create 真正例；允许合法同型置换并核实际结果 |
| 活窗口 | live Desc/loan 下移动或换 tag、观察当借入逃逸、错误 Store 身份 | 无 loan 完整 take/refill；借用结束后替换 |
| 闭包边界 | 未用非法定义、by-value 环、StoreElement/capture/array/borrow/header 越权 | Product<Enum<ErrorUnion<Product<Store>>>> 的有限实际分支 |
| 失败清理 | 半构造、后实参失败、半发表、实际 alloc/reserve fail | 第2/3 payload fail，独立/self RHS 区别，O0/O2 live allocations=0 |

每个 raw 模块先直接 Verify（原始与 Wire replay），不能把 Analyze 通过当 Verify 的测试前提；两核各有自己的正/负例与 shape/tag/source 求证。`Unpack` 的 local-defs 也需直接测试 ScopeExit、StorageDead、backedge、join。

预算在增长前预付：shape/type 拷贝、variant/路径/来源集合、目标去重、每次 CFG 状态与 join、DropPlan/layout、分支/helper emission。只按每 Sum 可行 alternatives 累加，禁止全局 tag 笛卡尔积。新功能检测应搭载已有有界预费遍历，不能为了旧 N 不变增加未付费整树 `.any`。各核精确 N / N-1 / fresh 重试，旧 CopyEnum/旧ErrorUnion/无新形状费用控制逐值不变。公共 Wire/schema/fingerprint、旧 unknown opcode 反例和 CT whitelist 需机械迁移；保留旧版本拒绝证据，并补当前 schema 的真实恶意 replay。

## 7. 本轮实际核对与限制

只读核对了 Q HEAD、作者完整树外方案，以及主树 `zil-types/src/enums.rs` 的 Coverage/pattern_test/match_statement、`zil-ir/src/lib.rs` 的 CorePattern/CoreMatchArm/is_copy_value、`zil-types/src/aggregates.rs` 的 Enum resolver、`zil-verify/src/lib.rs` 的 MakeVariant/VariantTag/ReadVariant。当前声明仍 Copy-only，owned Sum 尚不存在，所以本文没有 Pass 或实现缺陷结论。

root 已选定四指令 local 源/typed-path/递归模式和保持实际初始化年龄的最终发表协议。建议正式工单明确余下签名边界：路径只沿真实 Enum、输出 arm scope、ordinary move 不复活 stable shell，以及完整来源/实际 tag DropPlan。没有 guard 前置要求，没有继承父年龄的新权限。
