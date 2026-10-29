# 202静态接口审计：一个State中的F/G绑定与声明模式

**状态：只读接口审计，没有202实现、源码编译、raw/Wire或native成绩。** 审计基于 `5c2fbc50135d6ca12ee0481153b954194ad39b9d`，不修改生产、规范、schema25或默认预算，不扩大200/201验收。本报告提出后续需要冻结的接口选择，不宣布新规则已经准入。

设计摘要 `13ea0962a2bf73893a0b03eac133456216856f3748152392c925e95028059c7d`，规范集合摘要 `4402e04fd45551cbdd8dff92c4ac51af8d8ddd6f87296c17ce3690f55359bff3`。

## 核心结论

`State<T,F,G>`的一个拥有实参可以提供两个**类型绑定**，却仍是一个实际参数、一个完整Package责任。196的两个非逃逸callback角色各指向一个真实Closure参数，不能直接把一个State参数复制为两份角色并消费两次。200的两个同F字段也不能替代F/G：同一类型身份可以承载两份值，但不同泛型声明还可能要求不同签名和调用模式。

现有 `Bindings([Option<ClosureId>;2])`已经足够容纳有限F/G的**有序**类型向量，包括 `[id,id]`。主要缺口在Core元数据对来源的解释、一次实参取值后的多绑定提取、单环境推导项、名义专门化和字段声明模式来源。末端TypedIR已有逐字段Closure类型、真实local/SSA、借入调用及消费指令；没有证据说明必须为F/G State新增一套运行时环境或调用指令。

## 当前唯一parameter角色为什么不能直接复用

[`CoreCallableParameter`](../crates/zil-ir/src/lib.rs) 保存 `index/generic_index/generic_name/kind/parameters/result`。196中index指向真实参数，generic_index指向callable泛型声明，两者本来就不同：两个参数可以同F，F/G也可以分别出现在两个参数上。公开Typed合同的 `CallableRole.parameter`、联合 `CallEvent.parameter` 同样按真实参数来源工作，不能把Package字段号塞入parameter。

一个 `state:State<T,F,G>`若仅追加两行 `{index:0,generic_index:0}`、`{index:0,generic_index:1}`，当前至少有四处不满足：

| 当前位置 | 5c中的实际约束 |
|---|---|
| Expand `generic_callable::template_with_state` | owning State要求只有一个callable类型；源面先拒F/G State |
| Types `helpers::validate_template` | `bounds.windows(2)`要求index严格递增，重复parameter被拒 |
| Types `Builder::call_helper` | 对一个实参以 `.find(bound.index==index)`只选首行，随后只绑定一个环境；直接循环两行求值会重复移动同一个State |
| Types `InferBuilder::new` | 参数入口也按index `.find`首行，建立一个ClosurePackage；第二个约束不会自然进入参数term |

完整State操作在专门化后没有普通callback helper合同；Analyze/Verify从Package字段和实际指令检查它。为了绕过这些入口而制造两个Typed CallableRole或空联合调用合同，会把拥有值边界与196同步不逃逸协议混为一体。

## 建议冻结的最小Core语义

首先明确区分**直接callback参数角色**和**拥有State参数提供的泛型绑定**。对有限两个callable类型，可选以下两种公开表达，建议优先显式区分来源：

- 给Core绑定增加来源selector，区分直接参数与Package字段/字段族；保留owner parameter index、generic_index及声明签名。可以使用有限的字段索引表，或从已核符号Package形状中导出同一generic_name的直接字段集合。实际State参数仍只求值、暂存和转交一次。
- 或新增专门的State绑定元数据表，保持196的CoreCallableParameter及真实参数唯一性原样。新表只陈述字段使用哪个callable泛型及其声明模式，不授予运行时权限。

现有字段理论上已能结合符号Package里的 `Named(F)/Named(G)`区分字段；因此“必须新增某个TypedIR字段”并非本审计结论。但若选择重复index并重新解释旧Core表，必须明确这是**公开Core语义变更**，不能伪装成放松一个私有断言。所有 `.find(index)`、排序/去重、模式一致性、快照计费、CT复制与拒绝入口都要一起迁移。采用新表/selector会改变公开Core形状，应由root按公共变更规则冻结schema/指纹域；不能沿旧身份静默接受不同含义。

[`zil-ct/src/source.rs`](../crates/zil-ct/src/source.rs)会复制并检查Core callable元数据；[`zil-ct/src/materialize.rs`](../crates/zil-ct/src/materialize.rs)及[`zil-driver/src/ct_program.rs`](../crates/zil-driver/src/ct_program.rs)有CoreFunction构造/资格入口。它们必须保持：CT不借新State绑定获得免检权限，materialize仍重建普通可核Core。现有Wire主要传输Typed模块及候选；即使Core升级不增加最终Typed载荷字段，也需核对版本头、指纹身份、旧schema拒绝及恢复文件，不能把“Typed指令不变”写成“公共接口完全不变”。

## 私有推导和专门化的必要最小变化

| 位置/字段 | 最小需要保留的信息及改变 |
|---|---|
| `helpers::Bindings` | 保留现有有序两槽。F和G都绑定同id时两个槽仍保留；不做去重，不把运行时值放入类型键。`TemplateKey=(ModuleId,NodeId)`仍区分不同函数声明及其bound模式 |
| `callable_fields::owning_profile/environment` | 从“所有bound同一个generic”改成已冻结的有限State绑定profile；一个实际Package读取完整字段一次，按每个generic槽独立提取并一致性检查id；普通字段和名义形状仍精确比较 |
| `helpers::call_helper` | 实参求值/暂存按参数执行一次；从这一个实际类型上提取所有State绑定；逐F/G核实际模式兼容性、参数/结果签名、自含捕获；不能因二者同id跳过第二个声明约束 |
| `helpers::instantiate_helper` | 当前owned分支只取 `bounds[0]`专门化并清空metadata；需要一次应用完整有序替换表，随后构建仍可追溯字段来源的私有模式信息 |
| `helper_plan::Shape` | `Package(Vec<(String,ClosureId)>)`已经能记不同字段id，但 `environment()`只有全部id相同时才返回一个id。改为从声明字段/selector提取指定generic槽；普通直接Environment分支保持 |
| `inference::Term::ClosurePackage` | 当前 `environment:usize`是推导期的源工厂/closure term身份，不是runtime值。改为有限有序身份向量与字段→generic槽映射，不能只把所有字段继续替换为同一`$callback`占位 |
| `state_term/known_term/bind_callable_argument/unify` | 一次建立/比较完整符号shape、每个槽的源身份和字段归属；member投影查其真正槽。分支join/赋值/返回不能只比较布局或一个环境id |
| `Builder::state_callable_mode` | 单个 `Option<(ClosureId,CallableKind)>`不再够用；必须从实际字段的**声明来源**取模式，而不是按最终ClosureId全局查模式 |

上述新映射、字段扫描、键及实例计划都须先计费；有限两槽可继续用固定容量结构，不因为“只有两个”免除实际字段/类型/名称遍历的计费。

## 同实际ClosureId、不同声明模式的混淆

考虑尚未运行的审计形状：F声明 `Fn()->i32`，G声明 `FnOnce()->i32`，但两个字段分别持有同一Fn工厂产生的不同拥有环境。因此具体类型绑定可能是 `[C,C]`，运行时值仍不同。

当前 `packages::package_callable_callee` 根据 `state_callable_mode.filter(bound_id==id)`选择kind。把它简单改成 `Map<ClosureId,Kind>`仍然无法区分两个声明：后写入的模式会覆盖前者，或者强行选能力较强模式。正确目标是在该泛型函数内，F字段可共享调用并继续保留责任，G字段按声明消费、清理且不得第二次调用。不能因为实际C具有Fn能力便把G当Fn用。

这里不是已观察的生产混模式bug：当前F/G State在前门关闭。本审计只说明**一旦开放新形状，沿用按id选择的算法没有足够信息**。

最小私有模式来源应在泛型替换前保留字段→源generic的对应，并随State的实际本地绑定/移动/投影传递；callee由 `(实际local,字段位置)`找到其声明来源。只按ClosureId、字段拼写，甚至只按最终具体Package类型建立全局表都不充分：当F/G具体id恰好相同，`State<T,F,G>`和`State<T,G,F>`在当前函数的源约束位置不同，替换后可能具有相同具体类型向量。要保留这些源声明区别，不能从擦除后的类型倒猜。

声明模式属于当前函数的泛型能力约束，不能不加区分地当作所有同类型State的永久运行时属性。具体函数外的调用仍按已冻结的可见类型/接口规则处理；本预检不创造新的全局mode标记。

## 完整T/F/G名义身份

[`generic_state::state_family`](../crates/zil-expand/src/generic_state.rs) 当前要求两个类型实参、唯一callable binder以及1/2个同binder直接字段。`Engine.callable_name:Option<String>`与`single_generic()`都只保留一个符号名；需要有限binder集合及有序槽，不能给F/G任选一个“代表名”。

建议保持两个层次：family键保留定义模块/原名义声明、所有普通类型实参的完整身份和callable槽的位置；具体State键再附按声明槽序排列的完整 `[F-id,G-id]`。F/G是否指向同id不改变槽数和次序；T即使phantom、不同名义但同布局或通过透明别名出现，也不能被只看字段布局的算法吞掉。函数实例仍以真实定义TemplateKey和完整Bindings识别，不按调用者顺序命名。

当前 `callable_fields::specialize`逐次替换一个符号并追加`:closure{id}`，`nominal_base`会去掉此后缀。不能简单连续调用两次：第二次可能剥去第一次身份，留下只反映最后一个泛型的名义键。应先计算完整替换表和最终键，再只发布一次完整Package名；所有嵌套Type位置沿原递归规则处理。

正控制还应覆盖alpha改名（F/G改X/Y）和不同绑定顺序的相同语义；负控制应覆盖相同布局不同T、`[C1,C2]`与`[C2,C1]`、同字段名但不同模块State、漏第二个绑定，以及同F重复字段被错误配给G。不能只测两个工厂签名明显不同的容易情形。

## 独立Analyze/Verify边界与TypedIR复用

末端 `Type::Package`已经逐字段保存实际 `Type::Closure(id)`，类型声明和环境定义已有实际kind/签名。两个不同id或两个相同id的字段均能在数据结构中表达；原MakePackage、BorrowPackageField、TakePackageField、DropPackageField、CallClosureBorrowed/Once等能分别指定真实位置和SSA。当前200中真正阻止不同F/G构造的是准入合同，而不是没有第二个字段的存储表示。

Analyze/Verify的200双输入构造例外都明确要求两个实际输入相同ClosureId。后续F/G构造必须各自核两份具体输入、有限返回字段、自含捕获、完整名义类型及真实责任，不能只删 `first==second`：还需保证每个字段来源与模式使用由实际IR核验，未使用输入仍清理，错误路径只结算真正已持有的责任。

对于只有一个完整State参数的操作，当前普通helper合同扫描只识别直接Closure/Address<Closure>参数；它不会把State字段变成两个196角色。应保持拥有Package的实际核路径，不能给一个参数发布重复 `CallableRole.parameter=0`，也不能将字段索引伪装为联合CallEvent参数。独立核无需相信Types的源binder标签来证明内存/责任安全；它必须从实际字段、权限、调用模式及剩余清理重建。源声明的能力限制则必须在Core/Types端正确保留，不能仅凭实际环境更强就提前擦除。

目前没有新增Typed指令的必要性证明；也没有新TypedIR/raw/Wire成功结果。公开Core表示及版本由root另行冻结，最终仍需schema往返、字段/模式/身份伪造与budget故障检验。

## 建议的有限正反例（全部未运行）

| 配对 | 要区分的事实 |
|---|---|
| F共享重复调用，G只消费一次；G重复调用反例 | 不同声明模式、逐字段责任 |
| 同工厂两份拥有Fn实例，绑定F:Fn/G:FnOnce；交换State源泛型位置 | `[C,C]`不能合并类型槽或模式来源 |
| 两个实际工厂不同签名，分别在各字段正确调用；互换参数/结果反例 | 每个泛型签名独立核验，不只核首行 |
| 一State参数经过一次局部移动再调用；重复转交反例 | 状态参数只求值一次，模式来源随实际绑定走 |
| 先准备callee，第二实参`?`失败；正常对照 | FnOnce先取出的责任、两字段及普通Managed准备顺序 |
| 单字段借入/lending后操作sibling；同字段/父壳失效反例 | 199位置规则不因多类型而变成全局id规则 |
| T phantom/同布局不同名义、跨模块同名State；alpha重命名对照 | 完整名义T/F/G键 |
| raw同类型不同实际local、错字段、共享伪造独占、漏第二cleanup | 类型身份不是实例权限；Wire独立重建 |
| 原196直接角色、原197/199/200同FState对照；新边界N/N−1/fresh | 不把拥有绑定迁移带回普通helper语义或抬额度 |

本报告没有新增消费者、测试成绩或性能数字。四个Rust目标均不能由接口审计判定达成；当前只提供一个可审查的后续设计边界，未证明产品净突破。完整语言 NotSatisfied、产品 NotRun保持。
