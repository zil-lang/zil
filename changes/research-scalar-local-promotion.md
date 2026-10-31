# 已核单块标量局部的机器值提升候选

状态：静态提案与控制设计，未实现、未编译、未测收益。基线为43037eb7b108ba864144979204b5c6d18636dac3。该候选由第六阶段O2审计提出，等待真实LLVM pass profile后由root决定实施；不将SLP或MergeFunctions窗口的成绩归给它。

## 已核事实与归因边界

`crates/zil-backend-llvm/src/emit.rs`的`FunctionEmitter::emit`在L883为所有非Unit局部生成alloca，L926为参数写槽；L1148的Load和L1176的Store使用该槽，即使参数只读、从未借址也如此。`zil-lower/src/lib.rs:132`保留VerifiedModule中的原TypedCFG；机器发射才插入checked成功块。

当前每函数已经只有一个`zil.trap`（emit.rs:955）；`trap_if`（L2222）复用它，额外创建成功continuation。signed div/rem把零除与MIN/-1合并成一次检查（L1822），不存在每个算术操作各造完整trap体的问题。不能借优化移除任何必须失败的边，或把trap改成裸unreachable。

正式scalar样本的既有真实产物`stage6/merge-screen-default-after/large-8192-O2-0-0-zil/program.ll:147463`中，`value`的一个total槽跨8192个checked成功块传递：初始化及每次累加共8193个Store，8192次累加及返回共8193个Load；每次helper调用仍在对应累加之前。每个`x+常量`helper另有一份参数alloca/store/load。上述是既有源码与IR结构计数，不是耗时测量。opaque arithmetic/body是另一组运行质量样本，不与正式scalar形状混称。

Rust1.99.1固定revision `58cf8f9c35b355199727627d07ae34a4e2846d67`的`compiler/rustc_codegen_ssa/src/mir/mod.rs:323`先做non_ssa_locals；L536的合格标量参数直接使用OperandRef::Immediate。`mir/analyze.rs:17`要求单定义支配全部使用，L94遇到再定义则转Memory。因此可以证实Rust有直接SSA参数路径，不能由此声称Rust的多赋值total天然免栈槽。`mir/block.rs:730`跳过已知成功的assert，但非恒定assert仍创建panic块；signed checked_binop仍使用overflow intrinsic（LLVM builder L602）。源快照与逐文件SHA保存在`/workspace/.zil-env/research/stage6/llvm-shape-audit/`，由研究归档收录后才作为持久证据。

## 最小候选与安全条件

限定为后端内部机器表示优化，输入仍先经过当前独立Verify与bind_lir。首片只允许Bool、U8、U16、I32、I64；不把整个is_copy_value集合当成标量，不纳入Unit、指针、Address、Desc、ForeignHandle、数组、聚合、Closure、错误联合或资源。

1. 只读参数：参数初始化视为函数入口的唯一定义；全函数零Store、零借址、零捕获/未知局部使用，且只作普通Load，才以原参数值替代Load。多个源码块中的只读参数Load可使用该入口值；这是入口支配成立的单独规则，不开放跨块可变局部。
2. 局部变量：全部相关使用须落在同一个原TypedBlock，且只包含明确白名单的Load、Store、StorageLive、StorageDead。逐指令模拟当前值；每个Load必须已有当前激活中的定义，Store更新为当时RHS的ValueId。StorageLive/Dead清空当前定义，不能把上一激活的值带到下一激活。非只读参数若第一次Load仍依赖跨次进入块的旧值，回退原槽；最小实现可直接排除这类可变参数。
3. BorrowLocal/BorrowLocalMut、TakeLocal、捕获/环境绑定、资源/清理指令、跨块读写和任何未分类局部使用均令该局部整体回退。资格判定不只检查LocalDecl.mutable，也不靠“未看到通常的借用”推断无地址暴露。机器层再检查候选slot的所有实际使用只为匹配类型的普通Load/Store；出现GEP、作为实参、其它使用或未知Op即回退。
4. 每个Load替换为该时刻的值，而非局部最终值。例如`old=x; x=next; return old`必须保留old快照。采用真正的operand别名替换并删除已消解的Load/Store/Alloca；不得用freeze代替Load，也不添加nsw/nuw、noalias、nonnull等事实。
5. 分支、调用、overflow intrinsic、比较、checked成功块、trap、错误传播和全部清理指令保持原先后。机器发射产生的checked成功块仅是同一TypedBlock内顺序的细化，不为跨源码控制流合并提供许可。不引入Phi、跨块SSA求解或新的支配树算法。

证明边界：合格槽是已初始化、不可观察地址的整数/布尔私有存储，白名单保证只有当前函数顺序Load/Store能观察它。按原TypedBlock指令顺序归纳，当前值始终等于槽内容；每次Load读取相同数学值。普通调用拿不到槽地址，仍在原位置执行；任何checked失败仍走同一个失败边，后继调用与清理不被提前。循环中的合格单块局部必须每次激活先重新定义；无法证明时保留栈槽。未初始化或非法输入仍由原Verify拒绝，不能让候选修补非法CFG后成功。

原提议的“把多个overflow条件OR后才trap”不在首片：正式8192 value在每两次累加之间有函数调用，延后检查会执行原本不应到达的调用。无副作用纯算术链可以另立严格候选，但不能用它直接解释或改写本样本。

## 最小写集与实现接点

- `crates/zil-backend-llvm/src/emit/scalar_locals.rs`（新增）：资格计划、源块内当前值/别名记录与每函数机器指令替换。对局部、值及指令只做线性扫描，使用函数现有ID范围的有界表；不做每局部全函数重扫、不复制整项目机器IR。
- `crates/zil-backend-llvm/src/emit.rs`：声明内部模块，在每函数emit完成、公开文本/CGU划分之前应用计划。原Verify/bind_lir入口保持。源码侧计划与机器侧槽使用检查均满足才消槽；不满足时保留完整原函数/局部表示，不能部分重写后失败。
- `crates/zil-driver/tests/scalar_local_promotion.rs`（新增，root接点）：真实source→当前Verify→机器文本/API→native控制；后端单元测试只补直接针对未知机器use/别名快照的必要检查，不能以手填IR代替source控制。

首片不需要改zil-lower、zil-ir/Wire、LLVM API model/builder/pass、Analyze/Verify、CLI、Cargo.lock或逻辑预算。可在后端用穷尽Op匹配替换operand，无需向公共机器model添加Alias opcode。相同机器Module同时提供文本参考与API输入；API preflight及LLVM前后verify继续执行。原object cache以实际机器items和当前compiler identity绑定（object_cache.rs:146、203），候选产物自然获得不同key；不得仅复用旧收据而跳过当前核验。

当前EmitOptions不接收优化级别，候选在共同发射层实施将同时影响O0/O2；若选择仅O2，须另明确策略传递、文本参考和身份合同，不能暗加环境变量开关。先保持共同路径并核对两级行为及完整成本，不调整LLVM优化质量策略。

## 真实source控制设计（尚未执行）

下列新增短源是待运行输入，不声称已通过；语法来源为既有narrow_constants、borrowed_closures、cross_function_borrows和byte_io真实源码。每项都固定原输入、原参考与候选，API/Text、O0/O2使用同一行为oracle。

| 控制 | 待核source/预期 | 机器义务 |
|---|---|---|
| 只读参数 | `fn f(x:i32)->i32{return x+1;} fn result()->i32{return f(41);}`，返回42 | f参数槽消失，仍保留overflow intrinsic/失败边 |
| 同块可变值 | `fn f(x:i32)->i32{var y=x;y=y+1;let old=y;y=y+2;return old+y;} fn result()->i32{return f(10);}`，返回24 | y/old合格槽消失，old仍为11 |
| 真实调用间累加 | 保留现成stage6_bench scalar族的128/2048/8192源码及opaque三族 | 调用顺序、checked失败边数量、外部符号/地址合同保持；source/TypedCFG身份不变 |
| 分支合流回退 | `fn f(flag:bool)->i32{var y=1;if flag{y=2;}else{y=3;}return y;}`，两输入分别2/3 | y保留槽；不制造Phi |
| 循环携带回退 | `fn f(n:i32)->i32{var y=0;var i=0;while i<n{y=y+i;i=i+1;}return y;}`，n=0/8返回0/28 | y/i跨块槽保留；包括零次迭代 |
| 地址回退 | `fn read(p:&i32)->i32{return *p;} fn result()->i32{var x=40;let old=read(&x);x=2;return old+x;}`，42 | x保留真实地址槽 |
| 捕获回退 | 直接重放borrowed_closures中`parameter_address_has_distinct_native_stack_instances`、`borrowed_and_move_environments_keep_distinct_semantics` | 借用捕获仍是地址；旧值/新值、不同调用实例不合并 |
| 初始化与激活 | 单块多Store的Load快照，循环局部重新初始化，未知机器use回退；原未初始化/借用冲突source和源派生raw攻击继续拒绝 | 无初始化值不允许占位零/undef，StorageLive/Dead不带入上次值 |
| 数值边界 | 同样式扩展u8/u16/i32/i64/bool，覆盖最大值、负值、保值转换与checked失败；复用narrow_constants及byte_io/arithmetic | 不出现符号扩展/窄常量变化，不绕过div/rem零除和MIN/-1 |
| 清理与错误 | 重放error_cleanup真实四模块消费者、拥有闭包及byte_io的原输入/失败路径 | owned/live槽不消，已有清理顺序及完整输出相同 |

失败次序另用无缓冲系统调用作oracle，避免putchar在trap后未flush造成假阴性。沿byte_io的既有`write`+noescape合同声明`write(fd:i32,buffer:&[u8;1],count:i64)->i64`；`emit(c:u8)`写出一个字节。目标函数为`fn f(x:i32)->i32{var y=x;emit(65u8);y=y+1;emit(66u8);return y;}`，main用getchar得到opaque输入并分别调用f(i32::MAX的源字面量2147483647)或f(1)。失败必须输出精确`A`并trap，成功输出`AB`并返回2；另将除零放在第二次调用前核同顺序。所有外部函数均为真实libc调用，不使用手写C替代ZIL业务代码。捕获/资源清理仍使用既有消费者，不把trap改成错误返回或追加析构。

缓存控制需包含：同源码第二请求命中但当前Verify仍运行；更改被提升局部的初始化常量必须失效；恢复原字节复用原key；不同编译器参考/候选身份不串用。LIR原始伪造/不匹配VerifiedModule继续拒绝。原阶段N/N−1/fresh逻辑预算控制按同源重放，新增后端分析成本计入完整请求，不调高默认额度。

## 决策与计量

先由真实API pass profile区分SROA/Promote、支配树、InstCombine/SimplifyCFG和大root函数耗时；这份静态发现不能当作耗时定因。功能与非作者审核通过后，root冻结候选实现/参考二进制，以相同Merge、SLP、CGU/closed及目标参数独占交错计量。记录完整CLI、preflight/builder/optimize/emit、RSS、对象与原生产物体积、运行输出及opaque运行质量；保留128小样本与不合格局部为主的资源/循环分布，不只报告8192最好单元。不重跑或重标已冻结Merge/SLP窗口。

四目标：安全仅有上述等价条件与待运行反例设计；运行/内存质量未测；语言表达和公共接口不变，新增后端维护成本须计入；编译成本尚无实测收益。没有建立任何可重复净突破，完整语言/产品门保持原状态。
