# 有限普通拥有 Sum：Q 上只读实施建议

状态：树外建议，尚未施工；没有修改Q、schema、工单或规范。第9节是root追加guard要求后的协议修订，优先于第3/4/7节最初的两个opcode/裸SSA表述；推荐总计4条新指令，具体源operand待root冻结。源码基线Q `e92508be996a190fa188d08319d37cc8b522bae6`。本文中的新语义须由root先冻结。

## 1. 已有能力与真实缺口

消费者的原源/CLI/实际报告：`/workspace/.zil-env/owned-sum-preflightQ/`。`CHECKS.json`、`CONCLUSIONS.md`、`NATIVE.json` 已冻结。01 void!flat Leaf、02 void!无Store的nested Root及owned InitError!void、04 InitError!Leaf两侧不同flat Package都已Verify1；可接受源O0/O2共6构建/14次执行，活动tag输出符合原声明。两侧拥有ErrorUnion不是全新能力。06重复消费仍Analyze E-UNINITIALIZED。

真正的相邻拒绝：03 void!Session{SubscriptionSet{Store<Leaf>}}在Types的ErrorUnion资格门拒绝；05 `enum MaybeSession{Empty,Some(Session)}`在Types resolver以`enum payload requires a finite Copy value`拒绝。均不是消费者语法或native偶然问题。

当前实际公共IR名是MakeVariant/VariantTag/ReadVariant；它们对应提问中的MakeEnum/EnumTag/ReadEnumField。MakeVariant本来就有Vec<ValueId>字段，不需要单payload特例。

## 2. 建议冻结的有限形状

普通完整拥有形状取有限、无by-value环的DAG：Copy | 已独立核准的Managed叶 | Product(fields) | Sum(variants(fields))。

- Sum包括普通Enum（Unit/Tuple variants）及现有ErrorUnion的两个单字段variant；两者保留现有名义及各自ABI。
- Tuple variant允许任意有界字段数，每字段可为Copy、Managed或完整Product/已核有限Sum。Tuple variant的字段列表不需要新增一个通用拥有Type::Tuple。
- Product可以直接持有完整owned Enum/ErrorUnion；其直接Sum字段与Package child一样有一个完整字段责任。允许取出完整Sum、consuming match、完整回填/替换。这样Session包装enum与拥有该enum的Root都能自然参数/返回/局部，而非仅顶层临时可用。
- ErrorUnion载荷沿同一普通完整形状门扩展，至少闭合消费者03及Product中Sum再作为error/success payload；保留原flat双owned、Unit/nested及直接Managed-success模式，不重做它们。
- 只有某variant含拥有字段的Enum才是非Copy；其中Unit/全Copy活动variant仍是一个完整的线性enum Value，但运行时没有额外叶析构。
- 不自动开放StoreElement、拥有数组、capture、shared/lending、resource header、FFI、async、generic enum或借入payload；各旧门仍独立。Managed资源头继续有限Copy。已有Copy enum及Copy tuple/array保持旧资格。
- Store作为Managed叶时仍独立核canonical sealed资源和旧StoreElement完整形状；不把Store<owned Enum>或递归heap类型顺带开放。

源面须明确：裸资源名R仍默认Desc<R>，不能因进入variant就升级权限。建议Tuple字段允许已有词汇`manages R`，Expand仅将该位置包成现有Type::Managed；`Ref<R>`不许经该声明升级。CoreEnum.fields仍Vec<Type>，不需新增字段。普通Package载荷例如Some(Session)沿旧Value规则，不加give；Managed构造/转交沿现有create/give规则。

## 3. 最小公共IR增量

保持Type::Enum、CoreEnum、MakeVariant、VariantTag和ReadVariant的结构。新增两个指令，root/IR作者管理schema/Wire/fingerprint迁移：

```
UnpackVariant { value: ValueId, variant: u32, fields: Vec<LocalId> } // 无SSA result
DropEnum { local: LocalId }                                     // 无SSA result
```

UnpackVariant是一次原子完整消费：

1. 源Value必须是完整、live的owned Enum；variant必须真实活动且索引存在。
2. fields与该variant字段一一对应，数量/顺序/完整Type全等，LocalId不得重复或越界；目的storage必须live且vacant。不能以同型字段交换实际来源，不能覆盖已有责任。
3. 一次消耗enum SSA责任，并一次初始化所有目的local。没有中途用户调用/可恢复失败/挂起。
4. 每字段的真实来源/Created或Incoming/Store身份逐variant、字段及深层路径搬运。Unit/Copy字段无拥有债，但仍是正确类型的初始化输出。
5. Unit variant用空fields也消费完整enum。对同一value再次Unpack/移动/打tag应按独立live规则拒绝。

这复用既有LocalId/StorageLive/Store类多local定义模型（如capture绑定），不引入通用多SSA结果协议。所有definite-assignment、scopes、resource/loan使用者需显式识别这些local输出。

不能把现TakeErrorUnion循环用于多payload：它每次已consume整个union，第二次take不是“下一个字段”。也不建议多次TakeVariantField加隐含状态再Finish：那会额外引入半拆开的SSA Sum及失败清理状态。原子Unpack避开该复杂度。

ReadVariant对owned Enum只能产生Copy字段；绝不读出拥有字段。若pattern有refutable Copy tests，可在拥有值仍完整时先作真实tag+Copy比较，所有tests成功后才Unpack。若优先保持首片最短，拥有payload子pattern限Name/_，Copy子pattern复用现有覆盖检查；不把if-let/while-let的Copy门隐式打开。

## 4. 构造、match和替换事务

### 构造

MakeVariant可沿原Vec字段处理任意N。Types逐字段按现有参数准备规则求值：每个已成功拥有字段立即进入真实prepared temp，后一表达式失败时逆成功准备序清残余；全部准备成功后一次commit→MakeVariant。完整enum析构按该variant字段声明序递归。当前make_variant只self.expr并收集ValueId，必须补transfer/staging，不能仅取消Copy拒绝。

### consuming match

match用完整转交取得scrutinee，而非复制；实际tag测试在值仍live时进行。任意反驳型Copy pattern先测试，失败边保持同一个完整enum供后续arm；成功边才原子Unpack。MakeVariant已知tag或CFG精确等值成功边可提供证据，但不能只信Types生成的候选或运行时trap。

Name整个pattern绑定完整enum；Wildcard整个pattern留一个隐藏完整拥有local在arm退出时清理。variant中的Name/_各得真实local；被忽略的拥有字段也进入隐藏local，不能漏析构。arm return/error/break/defer都沿既有scope退出。

建议冻结：variant字段绑定/隐藏local的清理注册采用反字段声明序，使arm退出未转出的payload按variant声明序清理；所有这些local在同一个Unpack事务成功时建立，不能把源码存在多个field误解释成任意初始化年龄。此注册顺序必须在Analyze和Verify各自重建，不能仅让codegen按预期打印。

### whole replacement及slot age

仅DropEnum+普通Store会沿当前TakeLocal/Store路径forget/re-age，不能宣称自然保持238 lexical shell年龄。最短推荐复用155的私有单字段Package slot思路：仅真正需要整值赋值的可变Sum绑定，在Types内落为Package{value:Sum}；源Name通过私有映射仍表示Sum，不改函数参数/返回Enum ABI。赋值复用现有字段prepare→DropPackageField→StorePackageField；self静态调用在实际唯一目标实参点取field0，旧slot shell仍在，失败准备/旧字段清理/新完整值发布沿238/155既有机制。

这是普通Product<Sum>能力的实际复用，不依赖保留字名字授信、额外permission属性或公共ReplaceSum指令。直接独立RHS失败保留旧活动值；self目标被实际转交后，后续实参失败清理真实prepared carrier，不捏造恢复旧值。同表达式其它实参不能偷偷访问被替换目标；沿238的有界References/静态callee/唯一直接target边界核。未成功构造不能清旧值。完整Sum字段take/refill使用普通Product字段协议。

若root改选直接Sum局部模型，须先冻结其shell/payload双状态及替换年龄规则，不能只加一个类型白名单后用Store擦掉原年龄事实。

## 5. 每个核必须独立保存的事实

### Types/Expand

- Resolver遍历全部enum/aggregate定义（含未用），拒未知/环/非法借入/资源头及错caller binder。保全实际完整Type的nominal identity，不按名字或layout猜资格。
- 单独普通owned shape门用于新边界；旧capture/shared/StoreElement/array/Sum profile不能因递归调用普通门而被反向放开。
- affine_type、transfer_expr、has_cleanup、discard、known_type/source inference、参数/返回、match binding、package field take/refill和实际resource effects同步。必须先保留既有ErrorUnion注入prelude，再处理完整owned Value。
- managed-slot式Sum slot只对实际新replacement生效，旧无新形状函数不增加整树扫描/AST克隆。

### Analyze

现resources::Shape对enum没有owned分支；MakeVariant/ReadVariant只作为一般uses出现；现ErrorUnion很多地方只留Incoming。新Sum必须有独立完整债及按variant/field/深叶的来源。

可以复用Product的私有路径Vec<u32>，但路径须经结构形状解读，包括Product字段、Sum variant和payload字段层；不只按最后一个field index归并。同型两Store仍是两个实际来源。inactive variant没有债；潜在leaf lanes只作来源，不为所有variant同时制造责任。CFG合并用每个Sum自己的possible-tag集合，不枚举多个Sum的笛卡尔tag积。

MakeVariant消费所有拥有实参，保各自来源；Unpack将准确选择的各field来源给各目的local；Store/TakeLocal及Package field whole转交复制完整Sum来源计划；DropEnum/含Sum的DropPackage只消除真实活动branch。调用摘要保完整路径，不能通过pack/unpack把Incoming洗为Created。

scopes/initialization、cleanup年龄、defers、strong-desc/Store窗口转换必须认识Unpack的local定义和Sum包含的Store。移动/销毁旧Sum时，存在真实payload窗口的最近反例须拒；不存在borrowed payload源准入也不能让raw地址/描述逃过独立核。

### Verify

独立重建上述形状、canonical Managed/Store元素和责任；不调用Types/Analyze证明，不信candidate tag/leaf routes。现Product Plan的平铺区间须扩为能解释Sum variant区间/active mask的私有结构，而非把所有variant lanes都当live债。

Unpack的活动tag证据至少允许构造已知tag或精确VariantTag(value)==variant成功CFG边。当前ErrorUnion checker要求直接单前驱；ordinary match可能从先前失败链进入，生成器可为Unpack单独成功块，保持最短证据合同。错tag、相反边、额外前驱绕过、同值重复消费、目标重复/非vacant/错type/越界、ReadVariant读owner、交换同型payload来源均独立拒。

公有候选/Wire重放不能添加权限。旧scope/defer/drop顺序仍按实际成功初始化/注册事实重建。

## 6. 原布局可复用；Product析构必须变成结构化计划

Enum已有`i32 tag + aligned max-payload words`布局；payload本就是Type列表。保留该ABI，不降为pointer/heap box/RC，不改ErrorUnion的`i1 + 固定error/result slots`旧ABI。

backend enum physical_layout/store_payload当前只支持Copy，需按已核实际Managed inline payload、ByteBuffer/Store的真实三字header、Product字段及有限Sum递归；全payload区继续先zero，再逐字段写，保padding/bool spare bits、Unit及align/overflow控制。不能把Managed当Desc指针。Unpack读取真实tag后按准确字段offset写locals及各自flag，一次交接责任，无用户调用或分配。

DropEnum：检local live flag，先清责任flag，再按实际tag分支，只析构所选variant；字段按声明序递归。非法tag显式trap，不生成LLVM UB。field如Managed Store则调用其精确element/drop helper。

关键通用依赖：现`emit/packages.rs::teardown_paths`是静态leaf path列表，无法表达Product里的Enum/ErrorUnion活动分支。为新shape生成私有结构化DropPlan：Leaf(path,teardown) | Product(ordered children) | Sum(tag location, per-variant plans)。Package whole/field drop、enum drop、ErrorUnion drop及native入口error cleanup都沿同一个后端结构plan发真实tag分支；每个核仍独立核资格。旧profile可保留旧path fast path/费用，不强迫全仓每个旧Product重扫。

LIR继续保留经Verify的CFG并绑定immutable capability；只增加新指令编码/支持，不能让raw LIR自带plan成为权限。main入口仍沿原Unit/i32或既有ErrorUnion boundary；若错误payload新增为普通新形状，必须清其真实tag/深层Store，而不是只扩Types。

## 7. 最小必要写集（root冻结后再施工）

- zil-ir/lib.rs、wire.rs、canonical encoder/display及Wire tests/schema/domain；UnpackVariant的operands/local-defs/DropEnum枚举机械迁移。无新Type/CoreModule metadata表；只2个新instruction。
- zil-syntax AST/parser（仅`manages` tuple字段源marker）；zil-expand/enums.rs/type_ref及必要resolver/反例。generic enum仍拒。
- zil-types aggregates.rs/enums.rs/errors.rs/resources.rs/addresses.rs/packages(.rs/product.rs)/lib.rs/inference.rs、有限sum shape/slot辅助；相应resource_effects/closure generic保旧资格入口。237 registry完整Type遍历须识别嵌Sum实际args，但不授新binder或StoreElement资格。
- zil-analyze budget.rs/resources.rs/resources/product.rs、scopes/defers/holds及必要interprocedural origins/initialization消费者。
- zil-verify lib.rs/errors.rs/resources.rs/resources/product.rs/packages.rs、cleanup/defers/borrows/loans及raw/Wire独立tests。
- zil-lower ordinary value/signature layout admission。
- zil-backend-llvm emit.rs、emit/enums.rs、emit/errors.rs、emit/packages.rs及必要resource flag/Store helper/budget/layout路径；共用机器builder供LLVM API与现Clang fallback。
- CT/newop whitelist、IR exhaustive match/codec/cache fingerprint作机械处理；不新增CT拥有执行能力，不开放FFI/closure/array gate。
- driver新限定source/raw/Wire/native/API allocator tests；保留消费者03/05原源及旧成功/重复消费源，不改examples/oracle。evidence独立存真实commands/source/artifacts/旧失败。

这是拥有闭环的必要写集，不是仅改Types enum门的单文件任务。公共IR由root指定唯一作者，schema/任务/规范由root管理。

## 8. 预算与最短验证矩阵

预算：新shape资格、字段数/完整Type复制、variant/tag/source表、Unpack目标索引/比较、CFG join/循环、结构DropPlan及重复emission/布局都在增长前预付；深度/字段/variant/path/bitset宽度使用既有限额。预付preflight顺便记录含新owning Sum的function/type bit，避免为了保持旧N新增未付费整树classifier。Copy enum、旧flat ErrorUnion及不含新shape的Product走旧费；新形状独立精确N/N-1/fresh，预算拒绝后的重试必须相同IR/判定，默认限额不抬。

作者最短正例必须含：

1. 消费者03/05原样晋升，保01/02/04/06的原接受/拒绝与原14native oracle。
2. 至少一个variant含两个拥有字段（其中两个同型独立Store/Product），另有Unit和全Copy活动variant；参数/返回跨module、complete move、Copy伴随、consuming match取走一field另field退出清理。
3. Product{enum}→取完整enum→match→完整回填/替换；该Product进入ErrorUnion error/success载荷，非仅临时顶层。嵌套两Sum分支独立，不能扁平选错tag。
4. 构造第二/第三payload失败：只清成功准备的前缀，旧赋值variant保持；完整转换成功先准备后清旧branch。self call实参失败前/后、defer+later资源保持原序。
5. Copy pattern反驳后再下一arm，先验证tag/tests再Unpack；whole Name/_、payload Name/_遗漏部分及两字段相同type均有真实析构oracle。
6. source最近反例+raw/Wire：错tag/相反edge/绕过前驱、重复move/unpack/target、错arity/type、owner ReadVariant、Incoming→Created洗白、同型来源交换、live窗口跨变体切换、非法borrowed/capture/StoreElement/array资格、未用定义/深度/递归。
7. LLVM API O0/O2真实对象/输出/exit/Drop顺序；actual allocation injection覆盖payload准备及Store reserve失败，要求live allocations回0，不把手写C析构模型当native证明。独立参考后端需按root门执行。

未测：这是一份只读设计；未声称拥有Enum或Product<Enum>已可编译，未运行新的Rust测试/全量，未修改生产树。

## 9. root补充的guard/早退责任：修订最低协议

本节修正第3/4节“只由SSA一直保管到最终Unpack”的初稿，不能把该初稿当成已经足够的事务合同。

实查Q：Syntax `MatchArm`与Core `CoreMatchArm`目前只有pattern/body/span，没有guard字段；并且Analyze普通资源流明确拒绝对owned Package执行Load，capture view是另一条专门路径。因此不能声称已经存在可直接复用、可传任意消费者的通用Product/Enum view。若下一片包含标准`pattern if guard => body`源面，须同时冻结Syntax AST/parser、CoreMatchArm.guard:Option<CoreExpr>和所有walker/编码的有限迁移，不能默默把guard限制成纯表达式，也不能假称Q已支持该语法。

### 推荐：隐藏完整local保管责任，显式本地观察

在任何tag/Copy pattern/guard之前把完整scrutinee存入真实隐藏Enum temp，作为match所属scope的一项真实拥有局部。guard可以执行普通允许的调用、`?`或return；其早退走原scope cleanup，DropEnum只清真实tag。guard false继续下一arm，hidden owner保持完整；只有选定arm后才消费它。

为此推荐将最小新指令从两个修订成四个：

```
VariantTagLocal { local: LocalId } -> i32
ReadVariantLocal { local: LocalId, variant: u32, index: u32 } -> Copy payload type
UnpackVariant { value: ValueId, variant: u32, fields: Vec<LocalId> }
DropEnum { local: LocalId }
```

前两个只是完整live本地owner的观察，既不返回owner也不返回Address/Desc/任意可转交视图。ReadVariantLocal必须是Copy字段且在读取时满足实际活动tag和存活；其结果是Copy快照，后续使用标量不延长payload借入。原VariantTag/ReadVariant保旧Copy路径。guard可使用已经安全取得的Copy绑定及其它普通作用域值；若要求guard直接使用未转交的拥有payload绑定，需要另外冻结借入payload/read-view合同，不能在本片“no borrowed payload”下偷偷获得这个能力。

### 真实版本与短走廊证明

保存旧Tag的i32数值不等于保存旧payload权限。任何Store/Take/Drop/StorageDead使本地位置旧版本证据失效；类型或local名称相同、tag值碰巧相同不能恢复旧来源。Independent Analyze/Verify需按实际CFG重建这一点，不能读取producer私有“version”标记。

最短生成策略：任意effectful guard成功后，再发一次VariantTagLocal(hidden)并比较所选variant，进入唯一成功块，再TakeLocal(hidden)→Unpack。false边显式trap而非LLVM UB。这样无需将首次pattern tag证明跨任意guard传播。Verify可沿既有ErrorUnion直接选边风格，只核最终TagLocal定义、eq常量、唯一成功edge及Tag后/Take前不修改源local的有界短走廊；Unpack的value必须是同一实际local在该走廊的那次Take结果。raw在Tag后重填别的variant、或复用旧Tag/value、相反/绕过edge都应拒。即使codegen另有runtime tag trap，也不能用它代替静态拥有提取资格。

一个待root选择的更小operand合同是把UnpackVariant源直接改为local（而非value）。source match既然都先落hidden temp，此指令可原子清该local full flag并填目标locals，省掉TakeLocal与本地Tag的关联证明；不改变字段数和无中途失败语义。若采用此版，源/目标local必须互异，TagLocal证据直接绑定同一local/版本。这里没有预先修改任何公共声明。

### 可选但不推荐的省opcode方案

也可把Load(owned Enum local)定义成只许VariantTag及Copy ReadVariant消费的非拥有view。它需要每个核按SSA定义独立区分同一Type的owner/view、逐消费者禁止Store/Call/Return/MakeVariant/Unpack转交，并追踪local版本/liveness；否则会把物理复制误当复制责任。这能复用两个旧观察opcode，但实现面与raw攻击面比两条显式local观察更宽。仅为减少opcode数量不值得混淆线性owner类别。若root选该方案，应单独冻结并独审完整view协议，不能当一个Load白名单小改。
