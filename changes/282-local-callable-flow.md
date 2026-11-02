# 282：具体局部 callable 的实际角色流

承接 [277–280 合同](277-280-scoped-concrete-callable-composition.md)，基线 `2013bee5`，前置277精确提交 `a4608818`（本树 cherry-pick `c75c9821`）。生产首片 `2842b652` 已交280/278联调；本片不改公共IR、Wire32、默认额度或产品门。

普通泛型helper中的实际 `MakeClosure` 触发 Analyze 已有的私有 `forward::prove`：按真实CFG位置证明来源、移动、借用、声明调用模式及生命状态。依赖扫描原已预付的指令访问发现构造点；新增标记表、传播、状态复制及实际构造位置快照均按请求共享额度预付。构造处的callable捕获必须是该位置证明的具体fresh来源；符号角色、未知与混合来源拒绝，包含相同ClosureId的不同实际值。F进入Package/Union等不支持操作仍由该证明的实际操作数审计拒绝。普通借入数据捕获继续沿原资源/借用核检查，不新增self-contained要求。

子helper须先独立核验自己的真实合同。实际具体局部实参映射为无父角色，展开时过滤其call事件并保留子Return/Trap及Flow边；真实F映射仍保留调用模式、顺序和次数。具体ClosureId只核对实际签名，不能替代来源。同型fresh值不能复活已消费F；声明FnOnce不恢复实例较强的Fn权限。实际局部值的拥有、窗口、重复消费和剩余清理由原资源核检查。Analyze不调用Types或Verify的求证函数。

## 真实预算变化

旧费用在前置277提交 `c75c9821` 的隔离 `/tmp/zil-282-cost` checkout 上运行同一raw builder测得；新费用在282生产上测得。精确测试固定旧控制值，并检查当前N/N−1/fresh。

| 具名raw输入 | 旧N | 新N | 实际工作 |
| --- | ---: | ---: | --- |
| `fixture(Fn,false,1)` | 1548 | 1548 | 无新helper局部路径 |
| `scoped_helper(Fn)` | 1219 | 1219 | 无局部构造，不新增快照 |
| `scoped_helper_with_local_body(FnOnce,true)` | 2341 | 2352 | MakeClosure前10个local/value来源槽快照，另1个实际capture访问 |

新增无With输入的raw与Wire费用：直接局部调用1470、子helper转交1934、捕获实际局部handler2014。每个输入均N通过、N−1返回ResourceLimit、fresh N与原候选相等；没有重置请求额度。

## 无本地构造的公共工厂近邻

同一合同亦覆盖普通generic helper仅调用已有普通工厂取得具体local，再交给子helper的情形：helper自身没有With或MakeClosure。依赖扫描已预付的实际Call访问只在结果类型与已核公共`Fresh`返回合同及callee返回类型逐项一致时，启用原`forward::prove`实际CFG位置图；标记及传播预费。`Parameter(k)`不触发Fresh资格，原值返回继续替换实际实参来源，不得伪装具体local。未知/混合来源、真实F捕获、同型值替换及声明模式防线保持。

限定raw/Wire新增2项覆盖合法前缀、缺合同、Parameter伪称、错误实际结果/合同身份和local重复消费；具名`factory_only_composition_282()`精确N=2073，N−1为ResourceLimit，fresh N候选相等。旧普通1548、无local helper1219、scoped借入local2352及新增原三项1470/1934/2014均保持。复跑`concrete_local_282`8项、`scoped_`12项、`forward::tests`14项及Analyze Clippy均通过。

277前端补片`8087e344`（本树依赖cherry `76b57102`）消费后，实际命令`cargo run -p zil-cli -j2 -- check /tmp/282-factory-only.zil --stop-after analyze --json --emit-dir /tmp/282-factory-only-analyze`返回Success/phase analyze、diagnostics空、verification_runs=0；Typed/Candidate由CLI实际产生。原字节由280冻结于`tests/foreign-scoped-callback/callable-composition/frozen/factory-only.zil`。其普通generic `run`仅`make()->impl Fn(i32)->i32`再`dispatch`，F调用及返回后使用仍保留；本片不声称该邻例达到Verify或机器执行，280继续消费。

## 独审近邻补齐：有效Parameter与旧With-body角色捕获

独审指出前项`Parameter(0)`变异没有真实形参，只证明非法合同早拒；不把它计作有效原值返回的实际来源证据。追加独立raw正前缀`parameter_forwarded_local_282()`：callee实际拥有形参、`Parameter(0)`返回、精确`Returned`角色、空调用trace及`TakeLocal`返回均有效。相同ClosureId的符号FnOnce F与公共Fresh具体Fn并存；具体local经keep返回后调用、实际F经keep返回并保留唯一FnOnce事件均raw/Wire通过。抹去F事件在Analyze以`E-ANALYZE-IR`/`public helper trace claim differs from its CFG`拒绝；把已消费F替代同型Fresh实参以同码/`callable instance is moved, absent or cleaned on a reachable path`拒绝。这两项错误均发生在有效callee合同已核之后。

278独审发现旧With-body16路径可把声明FnOnce、实际Copy Fn的supplied B捕获到新body，再以Fn调用1/2次并抹掉外层B事件；该原Verify接受属于278独立核的失败，由278修复，不重标为Analyze接受。Analyze持久raw从合法`scoped_helper(Fn)`正前缀建立相同结构，1/2次变体均raw/Wire在实际MakeClosure处以Analyze/SafeReject/`E-ANALYZE-IR`/`forwarded symbolic F cannot be hidden in a new local environment`拒绝。另用审核者原`hidden_b(twice)`及其原fixture构造的精确module，直接调公开Analyze API与Wire解码Analyze，得到同一拒绝；未调用Verify求证函数。

本收尾仅增加2项持久测试与说明，无生产接受性、预算或ABI变更。限定命令`cargo test -p zil-analyze concrete_ -j2 --lib -- --nocapture`15项、`scoped_`12项及`forward::tests`14项通过（过滤集合存在交集，不相加作独立总数），Analyze Clippy通过；原不相交普通borrowed body通过，旧预算与factory-only2073保持。本片没有新增机器或Rust性能实证，四目标及产品突破裁决保持上文限制。

## 限定验证与分账

工具链 `/workspace/.zil-env/env.sh`，`CARGO_TARGET_DIR=/workspace/.zil-env/target-282`，jobs2：

- `cargo check -p zil-analyze -j2`：通过。
- `cargo test -p zil-analyze concrete_local_282 -j2 --lib -- --nocapture`：8/8。独立raw及Wire32覆盖局部直接/子helper/被捕获调用、F+local混合子角色、同ClosureId不同实际root、实际F捕获、未知及CFG混合捕获、声明Once复用、local重复消费、篡改F事件和kind升级。
- `cargo test -p zil-analyze scoped_ -j2 --lib -- --nocapture`：12/12，含既有不相交Address借入、实际窗口、失败准备、local owner及精确额度恢复。
- `cargo test -p zil-analyze forward::tests -j2 --lib -- --nocapture`：14/14，含既有实际F隐藏到State防线、同型返回替换、循环/分支、捕获窗口和私有slot。
- `cargo clippy -p zil-analyze -j2 --lib --tests -- -D warnings`：首跑因forwarded第8参数触发Clippy；删去重复布尔参数，改读本函数已证明的标记后重跑通过。失败保留，不计为语言反例。

280目录 `tests/foreign-scoped-callback/callable-composition/{frozen,observed}/*.zil` 的13个真实输入均由本树CLI `check --stop-after analyze --json` 通过，verification_runs均0。关键原字节 `frozen/handler-without-with.zil` 由旧 E-ANALYZE-IR nested环境门晋升Analyze；包含wrapper真实F先调用、具体Event→Reply handler再经普通dispatch及F返回后使用。`frozen/body-dispatch-capture.zil` 和 observed 同例也通过Analyze。阶段Typed/Candidate由实际CLI产生，临时位于 `/tmp/282-without-with`；280负责固化源/阶段接口、机器布局、生产Lower、O0/O2及逐分配点失败的交付证据。本片不声称这些机器验证已由282执行，也未跑正式全量。

## Rust四目标及突破

安全实证限于上述有限真实来源、模式及raw/Wire近邻；完整语言安全未证明。沿原内联ABI，无新增隐式Box/RC/锁；本片没有Rust运行/RSS配对。普通helper局部handler组合实际增加，框架维护成本未测。新增工作计费并保留旧控制，未测Rust编译性能。原失败源现达到Analyze是具体能力增量，产品四目标合取及可重复净突破未建立；B0、完整语言及产品门状态不由本片晋升。
