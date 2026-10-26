# 107 输入闭包审计

本文件随先行提案冻结，绑定派工基线b9cce1d/生产24ca。它审计实际生产闭包，
不是以旧缓存假设为证明。源码入口：
[session.rs](../../../crates/zil-driver/src/session.rs)、
[project.rs](../../../crates/zil-driver/src/project.rs)、
[ct_program.rs](../../../crates/zil-driver/src/ct_program.rs)。

## 需要证明什么

固定一个编译器可执行文件。对每个成功生产者，如果完整规范输入和有序依赖所
承诺的实际raw值相同，那么产出的raw值相同。这里的相同包含Span/节点编号、
模块次序、source map、接口和预算影响，而不只包含最终程序返回值。

叶子由完整SourceUnit和Limits构造；非叶子由其完整key承诺的请求信息与前驱raw
值构造。按生产依赖顺序归纳，可用“构造身份”标识这些未核对象。它允许不同
构造得到相同值时仍使用不同身份，不能把不同有效输入错误合并。指纹压缩仍有
通常的碰撞抗性前提；这里证明的是字段闭包和确定性，不是假称hash数学可逆。

## 键的真实输入

`session::query_key`的source域为`ZIL-DRIVER-SOURCE-2`：FileId、ModuleId、UTF-8
path长度/字节、source文本Fingerprint。QueryKey另外包含module、phase、rules、
target。rules为固定RULE_VERSION、SCHEMA_VERSION、crate版本和raw-session-v2，
随后按原顺序编码全部11项Limits及build_inputs。

Limits清单：max_source_bytes、max_tokens、max_nodes、max_functions、max_blocks、
max_instructions、max_locals、max_values、max_analysis_steps、max_ct_steps、
max_call_depth。不同限制可以保守地产生不同构造身份；低限额的拒绝不允许被高
限额旧快照绕过。

`project::project_fingerprint`的独立域为`ZIL-SOURCE-PROJECT-1`：root、模块数、
按实际顺序的各模块ModuleId/FileId/path长度和字节/文本指纹，再加paths条目数。
每个paths条目编码完整段数、每段长度/字节以及目标ModuleId。BTreeMap的确定顺序
保留映射，模块Vec顺序不重排。项目key用此指纹替换source input，同时继承完整
rules，并以真实root模块ID标记key.module。改变root、次序、别名目的地均已绑定。

TargetProfile::fingerprint编码id、triple、pointer_width、endian、backend、
rule_version和checked_arithmetic。Parse/Expand没有读取target，故其key.target
为ZERO；CT/Types/Analyze读取完整目标，故使用该指纹。不能只用目标名字或指针宽。

## 九个成功闭包逐项展开

### S1：单源Parse

调用`parse_with_limits(source, limits)`。SourceUnit来自同一借用；key包含该source
的全部四字段和所有限制。默认AttributeContext是当前二进制固定默认值，不从
外界配置取值；显式上下文解析API没有接入此Session入口。

lexer依次扫描UTF-8、parser分配本次解析的节点编号，属性检查消费得到的AST与
固定默认上下文。成功ParseResult的module、tokens、deferred_attributes、cfg_removed
均由这些输入决定；diagnostics非空立即返回Err，恢复AST不能发布为成功。

### S2：单源Expand

调用`expand_with_map_limits(source, parsed.module, limits)`，随后用同一source及
limits验证source_map。source全字段已经入key，parsed来自S1并作为有序唯一依赖。
名字解析/泛型/闭包展开只遍历有序AST和确定集合，不另读文件或环境。source_map
也是返回raw值的一部分；其所需原文/位置已绑定，不能只保存Core语义输入。

### S3：单源CT

先`infer_source_closures(expanded.core, limits)`，再
`ct_program::single(inferred, target, limits)`。源Core由S2承诺；目标、预算全在key。
CT使用本次构造的Compiler/Collector、BTreeMap/BTreeSet/Vec队列和原有纯CT验证/VM。
它不能执行运行时FFI、动态字节存储或任意宿主I/O。被收集的普通函数正文来自同一
Core，不存在免费的外部函数体缓存输入。CT失败或资源上限错误不发布Core。

### S4：单源Types

`zil_types::check(ct_core, target, limits)`，唯一raw前驱为S3。类型/CFG/布局的目标
属性全在target，预算全在rules。所有可见声明、调用体、生成节点、source位置已
在ct_core内；没有另一个隐式类型注册表。返回TypedModule的公共fingerprint保持
原实现，107只更改外层Session账本的身份。

### S5：共用Analyze

`zil_analyze::analyze(typed, limits)`。单源来自S4；项目来自P4的`typed.module`。
依赖的是整个Typed/ProjectTyped快照；analysis_key克隆其完整key，仅改变phase。
因此没有丢掉项目额外信息，也不把相同导出接口当作相同实现。分析的局部solver
和预算由typed/limits决定；查到的thread-local计数仅有cfg(test)，不参与生产值。
CandidatePlan内部公共module fingerprint仍按原Typed内容产生。

### P1：项目逐模块Parse

`project.modules`按Vec顺序遍历。`remaining`从完整Limits开始，每个成功ParseResult
按真实tokens.len扣token和前端node配额；`max_tokens=min(remaining_tokens,
remaining_nodes)`。本次query_key使用该模块SourceUnit及这个完整parse_options，
而不是只用原总预算。前序模块顺序/内容的影响经真实剩余额度进入本次key。

本闭包的另一个分支是私有loaded_syntax，下节单独证明。二者成功结果须相同，
不能把私有Arc来源当作无需检查的权威输入。

### P2：项目Expand

`resolve_imports(project, parsed, limits)`在进入query前执行。它读取paths映射、
有序AST中的import，以及root/模块图和预算；这些均已入完整项目key或P1前驱。
`expand_program_with_limits(project.modules, syntax, imports, limits)`返回有序modules
与source maps，随后以真实project.root和同一project fingerprint构造ExpandedProject。
依赖Vec严格等于P1遍历顺序，不能排序/去重。映射改变但源文本不变也必须产生新key。

### P3：项目CT

先从ExpandedProject.core推断闭包，构造SourceProgramContext；按原模块顺序使用
Compiler{program,module,target}解释CT。remaining从完整Limits开始，按实际steps、
nodes、functions逐模块扣减。inferred program、module ID/顺序、target和初始Limits
决定这一序列及成功结果。最终CoreProgram的root与source_fingerprint也来自项目key。
这不是把每个模块的CT预算重置为无限，也不能让热缓存免除不同预算身份。

### P4：项目Types

`zil_types::check_program(core, target, limits)`。输入为P3完整CoreProgram，而非只用
导出名/签名摘要。得到的TypedProgram包含module、interfaces与root_functions；
依赖和项目key涵盖其全部输入。后续entry选择读取root_functions，但不改变本次
raw构造，且backend每次在独立Verify之后重新执行。

## loaded_syntax私有来源证明

只有`compile_project_path`能调用带Some(loaded_syntax)的`compile_project_request`；
公开compile_project入口只能传None。loader在本次请求内创建源向量，读取每个规范
路径一次，分配FileId/ModuleId并按发现顺序解析，保存同一index的Arc<ParseResult>。
直到成功返回，project及syntax不借给任何修改者；Arc不包含内部可变性。

loader和project pipeline的token/node剩余量都由相同初值和每个相同结果的token数
更新，max_call_depth等其余参数保持相同。loader的词法max_source_bytes设为该源已
经检查的精确长度；显式项目Parse用原总字节限制。差别只影响拒绝条件：lexer先
检查text.len()>max_source_bytes，不把上限写入token/AST，成功扫描逻辑完全相同。
loader已按唯一文件总量扣字节，validate_sources又按完整向量检查总量；两者通过
时每文件都在显式上限内。成功token、AST和属性附加值由同源/相同语法限制决定。

loader的路径规范化、别名和import发现结果已固化在SourceUnit/path映射中，再由
project_fingerprint承诺。查询闭包不二次读取磁盘。碰撞的emit/output目的地会在
raw发布前被ArtifactPlan拒绝；取消也在已有检查点拒绝。这个私有优化不允许调用者
提交任意声称“已解析”的AST。

## 不是raw输入的选项及边界

| 选项/事件 | 为什么不进入新的构造字节 |
|---|---|
| stop_after | 只决定执行到哪阶段；不传入任一raw构造器 |
| emit_dir/output | 控制保护/物化/发布；不改变raw值，阶段产物仍按本次请求写出 |
| entry/optimization/windows/clang | 仅在独立Verify之后用于lower/codegen/link；raw不缓存这些机器结果 |
| cache_dir | 磁盘候选仅为与当前文本逐字比较的源字节；没有raw反序列化渠道 |
| profile | Managed在任何raw构造前拒绝；当前成功闭包只有Native |
| cancellation/observer | 控制是否完成/发布，不提供成功值内容；闭包后的取消先abort |
| wall clock/OS/scheduler | 只进入外层report耗时/取消判断，不进入成功IR值 |
| snapshot.revision | 元数据发布次序；实际依赖绑定的是key/kind/observed，不是此计数 |

这些排除理由逐个来自调用点，没有把所有CompileOptions粗略当作“不重要”。若未来
某项进入raw构造，当前闭包证明立即失效，必须加入绑定及最近反例。

## 保留的安全和失败边界

Session仍只保存不可变未核RawSnapshot，不保存VerifiedModule或VerifiedCTIR。
`finish_typed`每个到Verify的请求都重建Verify调用；证书匹配的Typed内容fingerprint
没有变化。内部构造身份即使被测试伪造，也不能跳过这个调用。

query先begin，执行闭包，成功后检查取消，再计算元数据并publish；Err与取消都abort。
Database仍检查完整依赖闭包和active周期，发布失败会清除active。107不改这些规则。
当前作用域是成功确定性生产者；负缓存、外部注入的手写raw或跨编译器持久化不在
此证明内，也不能后来默默把它们纳入同一个内部域。

## 必须由实测补齐的部分

本文件是源码闭包论证，不替代运行证据。实现后必须按提案运行全部敏感性反例，
保留新会话与热会话的完整产物/诊断，并验证loaded_syntax与显式源图在成功和预算
拒绝边界相符。原digest消融和plain/count完整请求矩阵另行冻结；任何性能数只能
在root独占窗口按固定协议采集，不能由上述推理预先宣布Go。
