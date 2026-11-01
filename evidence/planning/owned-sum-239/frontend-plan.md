# 240前端拟写集（只读准备）

尚无240精确baseline/worktree，不改生产。按root当前唯一选择准备：4op由239提供；无guard；typed VariantProjection路径只含真正Enum的(variant,index)；全部模式测试先完成，再逐层Unpack到temps，最后整个pattern DFS声明序逆序真实Take→Store提交最终Name/_locals；各核按真实操作核年龄。旧Q方案的SSA Unpack/未来guard/直接反每层注册即足够等内容已过时，不据此施工。

## A. Syntax（实质写集）

1. `crates/zil-syntax/src/ast.rs`：建议将enum Tuple字段改为`Vec<VariantTupleField { ty:TypeRef, manages:bool, span:Span }>`。ownership marker属于variant字段，不给通用TypeRef新增任意位置可写的Managed语法。现Unit/Record结构保留。
2. `crates/zil-syntax/src/parser/items.rs`：enum tuple字段循环解析可选`manages`，然后原TypeRef；只在该声明上下文识别。bare R仍原TypeRef，既有类型列表其它位置不变。
3. `crates/zil-syntax/tests/impl240_*.rs`：新marker多字段、无marker旧parse、错误前缀/Ref/borrowed以及span/深度边界。

## B. Expand

实质：
- `crates/zil-expand/src/enums.rs`：仅manages字段生成既有Type::Managed包裹，保留完整资源Type；普通Package字段不改变PassingMode。源Ref等不能借marker升级。generic owned enum仍拒。
- `crates/zil-expand/src/generic_enum.rs`、`generic_check_enum.rs`、必要的`generic_check.rs`：适配tuple field结构并保原Copy泛型enum资格/构造检查；不让前置实例化抹除generic身份后从Types新门获得owning generic enum。既有Copy泛型enum不是全禁对象。

机械AST字段访问/复制/预算迁移（已用rg枚举现有Tuple consumers）：
- `modules.rs`（project类型名归属/改名）
- `ct_selection.rs`（类型遍历/选择）
- `generic_alias.rs`
- `generic_copy.rs`
- `generic_budget.rs`
- `generics.rs`（两个原字段walk）
这些位置保留manages/span，只递归原ty，费用只为真实新增marker/field结构预付，不能重构通用泛型引擎。

定向测试：`crates/zil-expand/tests/impl240_*.rs`，原普通/泛型Copy enum、imported声明名义、manages实际Core归属/错资源和未用定义、预算N/N-1/fresh。

## C. Types核心生产写集

### 新私有模块（名称可按冻结卡调整）

- `owned_shapes.rs`：有界普通有限Copy/Managed/Product/Sum形状分类与验证；返回已付费的new-shape/含Store/含Sum信息，避免在旧函数上加未计费整树classifier。Sum含Enum/ErrorUnion，Product可持完整Sum；不修改旧capture/shared/StoreElement/array资格。
- `owned_enums.rs`或`enums/owned.rs`：hidden完整Enum temp、typed path模式观察、全模式成功后原子拆到真实temps、最终DFS叶逆序发布及清理scope。Copy enum旧路径保原逻辑费用。Copy Tuple/Copy enum子树可在Copy边界ReadLocal后沿旧SSA模式递归，不把VariantProjection当Product/Tuple索引。
- `sum_slots.rs`：仅实际需要替换的mutable Sum使用私有单字段Package slot；私有map保持源Type与实际slot分离，不向源码暴露内部field。复用155设计及238精确实参求值钩子，不能仅把Name全局改成Member后漏掉self事务约束。

### 必需现有模块

- `lib.rs`：接模块；普通signature/binding/return/discard/If值门；mutable Sum slot dispatch；真实IR opcode emission；本片只改变普通完整值位置。
- `aggregates.rs`：所有定义（含unused）有限Sum/Product互嵌解析；manages声明资格；资源/未知/环/Desc拒绝；完整字段类型take/refill规则。保源privacy检查，内部ops仍用精确编译器来源而非名字/span免检。
- `enums.rs`：make_variant transfer+逐payload准备；普通owning enum构造/coverage/match入口分流；原Copy coverage与conditional-pattern路径不自动扩到if-let/while-let。
- `errors.rs`：同一完整普通形状贯通ErrorUnion载荷，含Store Product及Product<Sum>；保旧flat双owner/Unit协议/直接Managed-success模式；inject prelude不能被新hook抢先。
- `resources.rs`：普通cleanup/DropEnum和含Sum的Product字段；资源头仍Copy，不能把owned Sum当新资源头或Desc。
- `addresses.rs`：affine_type/transfer完整Sum及Sum slot读/取；不加一般Address/借入payload资格。
- `packages.rs`、`packages/product.rs`：Product<owned Enum/ErrorUnion>、完整Sum字段读/取/准备/Drop/回填；保持旧element/capture/etc gate分离。
- `product_replacement.rs`：新Sum owning字段参与内部take/drop/publication；提炼或有限扩展238 pending-input helper为实际slot field求值，保持其它source参数/字段的检查。References独立核静态callee/唯一直接self参数，不按名字放宽。
- `inference.rs`：普通sum signature/Term/型绑定、pattern names、source构造/返回/条件值与完整源shape；新遍历预付，旧无new-shape费用不改。
- `closures.rs`：仅共享source inference的AGGREGATE domain允许普通完整Sum；实际capture/callable/closure结果资格维持旧门，不因这个domain放行。
- `resource_effects.rs`、`helper_plan.rs`：source参数/返回/模式绑定实际来源/资源效果保真，尤其pack/unpack不洗成Created，同型字段跟实际Vec映射；只在必要分支适配，不另增宽泛helper/capture入口。

条件写集（只有实际差异证明需要才动，先告root）：
- `managed_slots.rs`：如复用其既有付费reassigned_bindings扫描/slot辅助，仅提取公用helper，保旧Managed生成IR/费用；不用整个managed AST rewrite给Sum隐授内部字段权限。
- `helpers.rs`、`store_templates.rs`、`helper_plan.rs`中泛型普通调用：新Sum外形的自然普通API需要真实完整类型/来源时有限适配；不放新StoreElement，也不开放owning generic enum。
- `aggregate_templates.rs`、`callable_fields.rs`：它们当前已遍历Type::Enum/ErrorUnion；先只读确认全args/phantom及绑定保留。若新Product<Sum>真实触发未覆盖身份边界，给root最小差异再改，不能按layout/名字合并。
- `program.rs`、`incremental.rs`、`scalar_shapes.rs`：只在opcode来源传递或既有scalar-only缓存资格确有新边界时有限迁移；Prepared/FunctionSession当前新形状应不得借旧scalar body复用绕过元数据验证。

## D. 240作者测试/证据

- `crates/zil-driver/tests/impl240_*.rs`、`crates/zil-driver/tests/impl240_fixtures/**`；必要Types单元测试放`crates/zil-types/src/impl240_*`并仅从本crate test模块引用。
- 消费者03/05字节原源晋升；01/02/04/06及原native oracle不改。
- 多payload（两同型独立Store/Product）、Unit/Copy variant、跨module普通参数返回、Product<Sum>→字段take/match/refill、该Product进入ErrorUnion。
- 完整递归pattern：尤其右nested拥有variant `Outer.Pair(a,Inner.Pair(c,d))`，遗漏Name/_混合后退出为a,c,d；先对整模式测试，失败边Outer必须仍完整。
- 只在成功后Unpack到temps；最终无调用/失败/早退commit corridor逆DFS真实Take→Store；覆盖arm内return/?/defer及剩余叶清理。
- 可变Sum独立/self替换，实参失败前/后、准备前缀失败、old/new真实tag清理及年龄；同表达式其它参数没有内部权限免检。
- Source反例和Typed raw/Wire攻击面由240作者写必要限定输入，Verify资格/IR path权威由239/后续核作者及独审负责。相同type目标交换本身可以合法，断言来源随实际Vec映射而非按名字判断；只拒错误create/旧Store身份宣称。
- `evidence/impl/240/**`保原源、命令、阶段IR、actualAPI O0/O2/失败注入以及精确预算。默认不抬；不自行全量、不改Cargo.lock/main规范任务。

## E. 明确不在240独占写集

公共IR 4op/VariantProjection、Wire/schema/domain/fingerprint及其它crate机械枚举迁移由239唯一作者；Analyze/Verify/Lower/backend由root派工持有。240不在这些crate提前补资格以使source“看起来已过”。未获准确baseline前不落生产编辑。
