# 329：有限普通 Copy record 变体及具名模式（已冻结有限合同及作者结果）

基线 `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`，依据主树工单329及 `_zil_design.md` §8.1.5、§16.6和既有语法附件。规范按root提交 `e28e08d8825085e9a3c6bae57d2a85d3a2418481` §15.2冻结，constructor shorthand按root明确裁定沿既有record grammar。下面分别保留基线、实现范围、真实失败和作者限定结果；本片尚须非作者审核及共同集成。本片作者 `/root/recovery_audit` 不担任329独审。

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

## 冻结的有限合同

1. 允许普通、有限、无by-value环的完整Copy enum含record变体。只要有record变体，本片要求整个enum的所有分支均符合既有普通Copy enum资格；不因此开放拥有record payload、Managed、Package、借入字段或拥有泛型enum。Unit/Tuple的旧独立profile保持。
2. 声明 `enum E{V{left:i32,right:bool},U}`，构造 `E.V{right:true,left:7}`；字段名必须恰好覆盖该真实变体的全部声明字段，每名一次，允许书写顺序不同。constructor沿既有record grammar允许`E.V{left,right}`，每个shorthand读取同名值；具名pattern仍必须显式`field:pattern`。不允许spread、默认值或省略字段。空record变体 `V{}`仍与Unit或Tuple构造语法区别检查。
3. 字段标签属于真实enum声明及其变体，不能按同布局、同字段集合、别名末段或另一变体猜身份。字段的可见性跟随可访问的enum/variant载荷；首片不开放独立`pub`字段修饰，不忽略该标记。字段及变体未执行属性沿既有明确拒绝；未实例化的泛型变体也不能擦除其属性。
4. 所有显式字段表达式严格按源码书写序各求值一次，之后按声明字段序装入既有Enum payload。不能把表达式直接排序到positional call。传播、trap和嵌套求值沿既有序列规则；本片没有新的拥有清理责任。
5. 具名模式 `E.V{right:r,left:l}`，以及字段内现有Copy子模式和递归变体模式，仅在已有match上下文使用；字段必须显式写为`field:pattern`，不增加shorthand。标签完整覆盖，一名一次；绑定名按整个pattern仍不可重复。`_`作为字段子模式可丢弃Copy值。无rest、guard、引用payload、普通struct具名模式或新irrefutable variant binding规则。
6. record变体只接受具名构造/模式；Tuple变体不接受record拼写。不能在丢掉字段名后令 `E.V(a,b)` 或 `E.V(a,b)` 模式误用record变体。穷尽性、scrutinee单次求值、binding作用域与类型资格继续由既有Types及独立Analyze/Verify处理。
7. 复用既有有限Copy generic enum特化、上下文/显式实参、名义与phantom身份规则。未使用的enum声明、泛型函数和pattern都检查字段标签、类型及资格，不等特化或调用后补证。模板中直接类型变量不因此授予函数参数Copy能力；复制仍需已声明的Copy bound。
8. 为表达嵌套普通Copy record的真实前提，root同批冻结并已实现enum类型参数的直接保留 `Copy` bound（`enum E<T:Copy>`），沿320逐参数核验：未用声明检查 `Box<T>` 的真实前提，实际及phantom实参也分别核Copy；其余trait/where/life/const enum binder仍Unsupported。这是明确冻结的额外接受性增量。无bound旧generic enumprofile保持其既有有限实例规则，不能借此给符号函数增加Copy权限。
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

- 真实源码乱序构造+乱序模式，各字段独立比较；有副作用helper记录`right`后`left`为21，匹配各值为1/2；空record、嵌套Copy和同布局不同名义/不同变体近邻；pattern shorthand、条件及refutable绑定保持本片外。
- 模块公开/私有enum与别名/泛型/phantom实例、显式和上下文实参；未用泛型函数中的错标签、错类型、重复绑定及缺Copy前提；补足唯一缺项的最近正控。
- 重复/未知/遗漏字段、record/tuple拼写互换、spread/default/pub修饰、资源/拥有/地址字段、by-value环及非穷尽pattern保持准确拒绝；不把Unsupported记为合法功能通过。
- O0/O2真实机器执行，既有typed/raw/Wire tag/字段索引错配仍独立拒绝；若测试手填IR，只按raw近邻报告，不能冒充源语言消费者。
- 单源与项目的work/nodes/实例预算N/N−1/fresh/冷暖，实际余额耗尽不能获得免费下游；旧无record控制保持其已冻结边界。只使用既有Cargo目标，root负责正式全量与非作者派审。

## Rust四目标及本片限制

本片拟补有限具名Sum表达和完整字段身份，不是完整模式或完整语言。安全只按独立核及真实近邻证据裁决；原生执行不证明Rust运行/RSS同级；字段可读性不等于框架维护成本优势；精确预算不等于编译提速。有限表达增量须实际贯通后记录，Rust四目标合取与产品级净突破仍未建立。


## 实现及最后缺口修复

生产检查点为`99f22e992`（初步名义字段/模式）、`803a126d9`（Ascribed、alias phantom、付费检测/累计额度）和`d308a74fefb4998ca9b7b9f4a4b3f75f93666871`（quoted pun和unused变体属性）。没有新Core/Typed/Wire形状、默认额度或Cargo目标。

- 新形状检测挂在已经付费的Life/AST预检中记录声明Span索引，只有新形状增长再收费；没有为每次constructor或模块再免费扫描全树。新索引、标签比较、源码序临时值、类型上下文和源映射沿Products实际余额进入单源/项目lowering及publish。旧无record/enum-Copy-bound路径不借此重置额度。
- 每个准备字段使用已有Ascribed保留声明的真实期望类型，防止Sequence临时值把合法`i64`裸整数字面量先默认成`i32`。未用`fn bad<T:Copy>()->E<T>{return E::<T>.R{r:7}}`仍真实Expand/Unsupported/E-UNSUPPORTED，传入`r:T`的最近正控成功；Ascribed没有把T升级为numeric。
- 规范化alias目标在移除前检查完整实参，包括未出现在布局里的`ByteBuffer` phantom。alias本身仍是条件类型配方，局部条件假设不发布为函数Copy权限；无Copy的函数通过`type A<T>=E<T>`仍拒，补T:Copy才接受。
- 新record字段可用已声明的具体record上下文，例如`E.Named{p:{n:7}}`；不按字段集合猜名义类型。附加探针`E.Old({n:9})`落在既有tuple constructor匿名record参数上下文缺口，仍Types/E-RECORD-TYPE，本片没有把该旧入口宣称完成；混合分支正控使用显式`E.Old(P{n:9})`，原失败输入在此保留。
- reviewer原090b使用`Stmt`及`#splice`接收列表，分别在Parse/E-PARSE-QUOTE-CATEGORY和CT/E-CT-CATEGORY拒绝，未计为语言实现失败。改成既有合法`Syntax<StmtList>`/`quote<StmtList>`/`#emit`后，reviewer6c6481d08的源在803真实Success/Verify1却输出100而非7；ordinary struct同根源在review97502a600也真实输出100。d308同时修两者：保成员标签，独立物化value Name并按词法作用域alpha；同一Environment在名称复制之前预付token/origin/scope查找工作、一个生成AST节点及临时名称字节。
- 原始quote缺口源（合法类别）：`enum E{V{x:i32}}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let e=E.V{x};match e{E.V{x:y}=>{return y}}}}fn result()->i32{let x=100;#emit generated()}`。shared ordinary record源：`struct P{x:i32}fn generated()->Syntax<StmtList>{return quote<StmtList>{let x=7;let p=P{x};return p.x}}fn result()->i32{let x=100;#emit generated()}`。作者固定后各O0/O2实际输出7，原期待不改。
- 静态属性疑点原`#[inline]`源实际在Parse/E-PARSE-ATTRIBUTE-TARGET先拒，不能作为越过新guard的证据。真正可达的`enum E<T>{#[cfg(false)] Row{value:T}}fn main(){}`在803实际Success/Verify1；d308补回旧generic enum等价的unexecuted variant.attributes守门，现Expand/Unsupported/E-UNSUPPORTED。

## 作者限定验证（最终生产d308）

最终命令均在`/workspace/zil-329`、`CARGO_TARGET_DIR=/workspace/target329`、jobs1运行；本片未跑正式全量、未push，未兼任独审。

- `cargo test -p zil-driver --test aggregates --test impl105_ct_enum record_enum329 -- --nocapture`：d308及ac2dc7deb均实际9/0；其中新增近邻涵盖9个test（aggregates8、CT1），包含合法field label/body/opaque身份、全字段/source顺序、上下文、未用定义/Copy、两种quote pun、CT runtime-shadow拒绝和raw/Wire三个攻击各双入口。
- 新增真实机器覆盖为11份程序、每份O0/O2共22次观察：单次求值/声明序、constructor pun和empty/Unit/Tuple共存、nested/ordinary record上下文、Copy/phantom generic、跨模块alias、CT值、生成函数及标签/值绑定分离，另enum/ordinary struct quote pun。重复回放不相加，不冒充LLVM API或Rust性能对照。
- full-request阈值为work2397/nodes860/instances3，N成功、N−1 ResourceLimit、恢复N成功，冷/暖/fresh一致且active queries归零。别名/engine费用取真实相加；source specialization753/290、project normalization/specialization1013/295，两种精确耗尽额度只够前段，下游public Expand必须非空ResourceLimit。
- 旧`impl240_generic_enum_budget`2/0，既有tuple_binding6/0，已有CT enum完整目标13/0（其中1为本片新增，重复不加覆盖）；定向Expand lib/tests及driver aggregates/impl105的Clippy -D warnings通过，改动Rust文件rustfmt和git diff --check通过。

首轮新增source测试3/3失败真实暴露Sequence丢i64 hint与unused alias phantom误放，已修；随后CT测试中`#(n)`常量本身的旧不支持及numeric-T反例误写SafeReject期待先后修正为实际边界，均未晋升源码。重写后的pure `#(n())`标签/绑定最近控，runtime binder实际Ct/SafeReject/E-CT-RUNTIME，保留函数名只作为label则机器返回12。quoted pun和cfg缺口由独审实际发现，作者修复结果不代替其独立复验。


## 共同探索A的真实符号身份阻断及修复

共同探索A `8941f1cda65aadd6e93d857b6961079d53a6d344` 的全仓通过
没有解除独审阻断。reviewer保留的未用定义在 `i64:Copy`、`bool:Copy`
等Type binder拼写下，被错误授予整数/布尔模式权限；Astra用A的真实
release CLI重放43固定源，29负例中21误接受且Verify1、8已拒绝，14
合法近邻全部成功。这是现行真实约束检查缺口，没有据此声称源码UB。

生产修复 `ac2dc7deb92217f4ec1bf9eb1798a8483ab9c8e6` 沿root正式边界
`changes/329-primitive-binder-identity.md`，在既有付费AST header遍历中
只观察真实Type binder的原生名碰撞；life名字不触发。无alias时从同一
剩余额度完整预付AST后复用既有作用域binder正规化，再走where与engine；
有alias仍沿既有保护。Products及项目路径继续传递真实剩余预算，没有
提高默认额度、公共schema或禁止合法同名binder。枚举模式的数字/布尔
资格同时要求非符号类型；alpha隔离保留callee返回的真实原生类型与声明
内真实原生字段，不按caller的同名binder映射污染它们。

作者构建的debug CLI SHA256为
`7442a2ccd7e34beb05a08fa7c8f330228a0da13b4cf55cf3988852639c4e359f`。
该源码树未合331混合life，固定源中29负全部Expand SafeReject或Unsupported，
12可用正控全部Verify Success；另外2个混合life正控明确待共同B，未冒充
43全过。独立审查由Astra/impl314负责，作者结果不替代独审。

ac2源码的作者限定回放：record_enum329九项9/0，record_enum329_ledger
两项2/0；原请求N2397/860/3及source753/290、project1013/295保持。
新增普通合法primitive-name binder实例化源的实际full-request work2713、
nodes666，两个阈值均严格低于默认额度，N成功、N−1非空ResourceLimit、
恢复N成功，冷/暖/fresh一致且active queries归零；此控制在既有aggregates
目标，无新Cargo target。上述N只绑定本作者树，含331/330的共同候选须按
其真实费用复验，不把作者局部数值冒充共同验收。限定Expand lib/tests及
aggregates/impl105的Clippy -D warnings、改动Rust文件格式与diff检查均实际通过。
正式全仓、共同B和远端CI由root裁决。
