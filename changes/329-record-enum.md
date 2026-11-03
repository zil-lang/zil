# 329：有限普通 Copy record 变体及具名模式（合同提案）

基线 `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`，依据主树工单329及 `_zil_design.md` §8.1.5、§16.6和既有语法附件。当前仅提案和基线探针；本文不表示已实现或通过。本片作者 `/root/recovery_audit` 不担任329独审。

## 实际基线

在独立树、`target329`、jobs1 构建 `cargo build -p zil-cli --no-default-features` 后，使用实际 `zil check <source> --json`：

| 源码 | 实际结果 |
| --- | --- |
| `enum Pair{Data{left:i32,right:i32},Empty}fn main(){}` | Unsupported / types / E-UNSUPPORTED / record enum variants |
| `enum Pair{Data{left:i32,right:i32}}fn main(){let x=Pair.Data{right:2,left:1};}` | 同上 |
| `enum Pair{Data{left:i32,right:i32}}fn read(x:Pair)->i32{match x{Pair.Data{right:r,left:l}=>{return l*10+r}}}fn main(){}` | SafeReject / parse / E-PARSE-PATTERN（qualified patterns require a constructor argument list） |
| `enum E<T>{Data{value:T,marker:i32},Empty}fn unused<T:Copy>(x:T)->E<T>{return E.Data{value:x,marker:7}}fn main(){}` | Unsupported / expand / E-UNSUPPORTED / record enum variants |
| `enum Pair{Data(i32,i32)}fn main()->i32{let x=Pair.Data(1,2);match x{Pair.Data(a,b)=>{return a+b-3}}}` | Success / verify |

另外，现有 `enum E<T:Copy>` 本身尚为 Unsupported（enum binder不接受bound）；不能将此失败误记为record变体失败。源码和报告临时路径 `/tmp/zil-329-probes`，准确输入已在上表保留。既有tuple模式不重做。

## 请求冻结的有限合同

1. 允许普通、有限、无by-value环的完整Copy enum含record变体。只要有record变体，本片要求整个enum的所有分支均符合既有普通Copy enum资格；不因此开放拥有record payload、Managed、Package、借入字段或拥有泛型enum。Unit/Tuple的旧独立profile保持。
2. 声明 `enum E{V{left:i32,right:bool},U}`，构造 `E.V{right:true,left:7}`；字段名必须恰好覆盖该真实变体的全部声明字段，每名一次，允许书写顺序不同。首片使用完整显式字段表达式，不增加shorthand。不允许spread、默认值或省略字段。空record变体 `V{}`仍与Unit或Tuple构造语法区别检查。
3. 字段标签属于真实enum声明及其变体，不能按同布局、同字段集合、别名末段或另一变体猜身份。字段的可见性跟随可访问的enum/variant载荷；提议首片不开放独立`pub`字段修饰，不忽略该标记。字段属性沿既有普通属性目标校验，不产生权限。
4. 所有显式字段表达式严格按源码书写序各求值一次，之后按声明字段序装入既有Enum payload。不能把表达式直接排序到positional call。传播、trap和嵌套求值沿既有序列规则；本片没有新的拥有清理责任。
5. 具名模式 `E.V{right:r,left:l}`，以及字段内现有Copy子模式和递归变体模式，仅在已有match上下文使用；字段必须显式写为`field:pattern`，不增加shorthand。标签完整覆盖，一名一次；绑定名按整个pattern仍不可重复。`_`作为字段子模式可丢弃Copy值。无rest、guard、引用payload、普通struct具名模式或新irrefutable variant binding规则。
6. record变体只接受具名构造/模式；Tuple变体不接受record拼写。不能在丢掉字段名后令 `E.V(a,b)` 或 `E.V(a,b)` 模式误用record变体。穷尽性、scrutinee单次求值、binding作用域与类型资格继续由既有Types及独立Analyze/Verify处理。
7. 复用既有有限Copy generic enum特化、上下文/显式实参、名义与phantom身份规则。未使用的enum声明、泛型函数和pattern都检查字段标签、类型及资格，不等特化或调用后补证。模板中直接类型变量不因此授予函数参数Copy能力；复制仍需已声明的Copy bound。
8. 为表达嵌套普通Copy record的真实前提，建议同批允许enum类型参数的直接保留 `Copy` bound（`enum E<T:Copy>`），沿320逐参数核验：未用声明检查 `Box<T>` 的真实前提，实际及phantom实参也分别核Copy；其余trait/where/life/const enum binder仍Unsupported。这是需要root明确冻结的额外接受性增量，不先实现。无bound旧generic enumprofile保持其既有有限实例规则，不能借此给符号函数增加Copy权限。
9. `#for`声明生成record enum仍保持既有明确Unsupported；本片不自动扩充§16.6。普通已核Copy enum的CT使用复用已有Core/CTIR管线；新增AST模式的预扫描、绑定、卫生和物化必须完整遍历、计费，不能丢标签或漏子绑定。
10. 全部新标签索引、比较、复制、排列、子模式访问、生成临时值及源映射在增长/访问前计费；当前请求names/alias/engine/module/lowering/publish使用实际剩余额度，不重置预算，默认额度不提高。无新record变体的既有路径不增加费用，除root另行具名冻结的必要实际工作外不迁移旧oracle。

## 生产接点与公共表示

- Syntax已有 `VariantFields::Record(Vec<Field>)` 和构造 `ExprKind::Record`。新增显式 `PatternKind::RecordConstructor{path,fields}` 与 `RecordPatternField{name,pattern,span}`，保存标签及子模式，不用positional包装约定编码名称。
- Parser在qualified pattern之后解析具名字段并保留span；声明字段仍沿现有Field解析。Expand/modules/alias/substitution/预算/CT扫描等AST walker逐项处理新节点，字段标签不做值绑定卫生重命名。
- Expand在真实模块/alias/generic实例上下文建源enum形状索引，声明资格与完整标签核完后才转入既有Core。构造用现有 `CoreExprKind::Sequence` 先保存源码序临时值，最后普通Enum constructor call按声明序读取。模式按已核真实声明映射到现有 `CorePatternKind::Variant` 的声明序字段；保留完整类型及变体身份。
- `generic_enum` / `generic_check_enum` / `generic_copy` / `generics` 同时理解record字段及具名构造/模式；不得只修最终非泛型Expand。未用定义、nested Copy obligation、原有构造与pattern形状区分都在名称丢弃之前检查。
- 公共Core/Typed Enum表示、Wire34及指纹域不变。原有Types/Analyze/Verify继续对全部类型、tag、字段索引、来源及实际指令核验；本片不会让raw Core字段字符串承担证明。
- 新AST节点要求driver的AST模式walker更新。预计窄扩写：`ct_artifact/{compiler_cfg,compiler_selection,hygiene,materialize}.rs`、`ct_selection/{context,hygiene,materialize}.rs`；仅完整遍历/绑定/卫生/计费或保持明确不支持，若实际无需改某文件则不改。root已批准上述窄扩写；按实际必要文件修改。
- 工单所列driver `enums.rs` 当前不存在，创建会新增Cargo目标；root已批准改用既有 `aggregates.rs` 和必要的 `impl105_ct_enum.rs`，不新增target。

## 限定正反控制

- 真实源码乱序构造+乱序模式，各字段独立比较；有副作用helper记录`right`后`left`为21，匹配各值为1/2；空record、嵌套Copy和同布局不同名义/不同变体近邻；shorthand、条件及refutable绑定保持本片外。
- 模块公开/私有enum与别名/泛型/phantom实例、显式和上下文实参；未用泛型函数中的错标签、错类型、重复绑定及缺Copy前提；补足唯一缺项的最近正控。
- 重复/未知/遗漏字段、record/tuple拼写互换、spread/default/pub修饰、资源/拥有/地址字段、by-value环及非穷尽pattern保持准确拒绝；不把Unsupported记为合法功能通过。
- O0/O2真实机器执行，既有typed/raw/Wire tag/字段索引错配仍独立拒绝；若测试手填IR，只按raw近邻报告，不能冒充源语言消费者。
- 单源与项目的work/nodes/实例预算N/N−1/fresh/冷暖，实际余额耗尽不能获得免费下游；旧无record控制保持其已冻结边界。只使用既有Cargo目标，root负责正式全量与非作者派审。

## Rust四目标及本片限制

本片拟补有限具名Sum表达和完整字段身份，不是完整模式或完整语言。安全只按独立核及真实近邻证据裁决；原生执行不证明Rust运行/RSS同级；字段可读性不等于框架维护成本优势；精确预算不等于编译提速。有限表达增量须实际贯通后记录，Rust四目标合取与产品级净突破仍未建立。
