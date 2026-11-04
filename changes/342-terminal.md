# 342 普通泛型借用与终态组合

施工基线：`77eea5bde60b1737b1a7f885ff549ed17308c3de`，派工权威：主树 `.task/impl/342.md`。

## 先行变更提案：定义期真实窗口

普通未实例化模板目前只检查可变 place 的类型资格；现有定义期 loan engine 仅在 owning callable State profile 中开启。最近反例为 `fn bad<T:Copy>(p:&mut T)->T { let a=&mut *p; let b=&mut *p; *a }`（即使从未调用，第二次独占借入也必须使旧 `a` 无法再次使用）。等价有用正例在旧 `a` 最后使用后才建立 `b`，允许原父引用在 child 最后使用后恢复。

实施拟复用现有计费的 loan identity / parent / invalidation / prepared argument / CFG join / backedge 引擎，覆盖所有普通模板；从实际参数引用模式建立独立 referent root，不从 life 名称、同尺寸或具体实例制造权限。普通局部 place、读写、移动、重借和调用窗口统一进入该引擎。type/life binders 的名字空间仍分开；有 life 时返回来源继续用真实参数索引核对声明上界，mutable result 还需真实 exclusive 来源。无 life 的模板同样不得丢弃别名错误。

随后在同一完整符号窗口证明上准入 `T:Copy + life` mutable 输入/结果。Analyze 与 Verify 仍各自重建具体 CFG 来源、返回权限和调用窗口；Core/Wire 现有来源合同不赋予证明位。新增扫描、loan、父链、根集与分支复制在扩张前累计既有请求预算，不增加默认限额。既有正例、最近反例、未调用定义、raw/Wire、实际 O0/O2 和 N/N-1/cache 依适用范围验证。

HRTB/借用 FP、借用存储/lending、显式 dyn 和一般 §5.2 状态后续按实际表示依赖继续实施；此阶段备份不代表该项终态完成。任何公共 schema/AST/Wire 变化先交由集成方发布，不能通过删除 Unsupported 或新增权限位替代证明。

## 首个实际里程碑：未使用普通模板别名核验

移除 State-only lane suppression，同时扩展来源演算：每个实际 Reference 形参由其真实 mutable 模式建立外部 referent root，root 与本地指针槽分离；普通值 local 为实际存储根。普通 local 的值读取、index 读取也参与窗口失效，不能靠缺失访问绕过已有贷款。父子链、同时实参窗口、分支版本化、循环 backedge 与预算复制复用原引擎。

实际限定命令（source `/workspace/.zil-env/env.sh`；独立 `CARGO_TARGET_DIR=/workspace/.zil-env/target342`、jobs1、debug0、incremental0、专用 TMPDIR）：`cargo test -p zil-driver --test generic_regressions --test cross_function_borrows --test impl197_mixed_state --test review184_storage --test impl200_independent --offline -- --nocapture`，先前单项失败按下列来源分账后分别完整重放：generic_regressions33/0、cross_function_borrows37/0、impl197_mixed_state14/0、review184_storage4/0、impl200_independent10/0。不是正式全量。

原 retained `mutable337/known-incomplete-unused-generic.zil` 字节不改；从未调用和有调用均在 Expand/SafeReject/E-GENERIC-ARGUMENT 拒绝。有调用路径原断点为 Verify/E-BORROW-CONFLICT，旧用例仅迁移预期阶段/诊断。首次回归运行保留该预期失败。另一次旧 exact budget 2358→2356 的暂时少费由保留 callable binder 访问收费修正，旧2358边界不变；不以调高限额处理。

331原源Expand N757→779，来自新真实参数贷款建立；其它符号/别名源本轮 N1414/275/1391，各N−1 ResourceLimit/fresh N一致。322原源99/95/714/993、337具体源143/124/1178/4818保持。现有借用目标含真实两核、Wire、原生、缓存回归；本里程碑新增2测试覆盖8份未用近邻反例与6份合法最后使用/分支/循环/独立参数正例。

Rust四目标：未用泛型别名责任增加可重放安全证据，完整语言证明仍待后续；沿既有地址ABI，没有新增隐式分配/RC/锁，未测同语义Rust运行/RSS；普通泛型无需实例化才能发现此类错误，尚无框架维护成本数据；新loan来源与复制实际累计收费，没有编译速度比较。实质增量为真实未使用普通mutable模板的既有漏洞闭合，不代表产品四目标合取或产品净突破；原product-gates/B0/完整语言状态不变。

## 第二里程碑：CopyT+mutable life 与完整返回窗口

规范扩展由集成方 `0feb9324c` 冻结，本分树按授权接为 `0954193ed`。实际 CopyT/life 定义先执行已扩展的普通贷款证明，再准入 mutable 来源；定义期显式返回上界按实际参数索引核对，mutable 结果还检查源形参真实模式。静态 helper 调用按该 callee 自己的 binder 和实际参数位置代换，返回窗口只取允许来源；参数位置是语义身份，未用 Span 或名称作为身份。

架构复查还指出 no-life 局部逃逸和 call-as-place 空来源两个近邻。所有引用返回现核已建立的外部参数根，局部值根不能逃逸；嵌套地址签名和借用在未用定义中仍明确 Unsupported。实际 helper 调用产生的贷款经 place 投影、index/range 端点及赋值 RHS 保留，不能被后续标量表达式清空。Store/ByteBuffer Desc 形参和原语建立真实外部存储根/借用窗口，使旧合法 lending 消费者继续通过；新局部存储不因此变成外部根。

新增9份真实 CopyT/mutable-life 源完成 Clang/API × O0/O2 共36次构建和执行，全部打印42/exit0，覆盖返回/重借/同拼写type-life/slice/记录字段/独立label/共享降权/分支/真实defer更新。6份未用反例、raw/Wire错误上界、跨模块CT及live/restored缓存的当前body/权限/调用方冲突检查通过。新增一般模板近邻包含call直接投影、局部逃逸、store来源、嵌套地址。两份337旧混合mutable源逐字节保留，从Unsupported迁为Success；原局部标注life仍Unsupported。

实际 `cargo test -p zil-driver --test cross_function_borrows --test generic_regressions --test impl197_mixed_state --test review184_storage --test impl200_independent --features llvm-api --offline -- --nocapture` 的生产变更已完整通过42/35/14/4/10（后续两项budget/cache测试在各自既有目标限定重放通过；完整重放交给集成方）。Clippy含两个受影响目标、llvm-api、`-D warnings`通过。mixed select的Expand/Types/Analyze/Verify N=1669/248/2227/7927，每个N−1 ResourceLimit且fresh N一致；331原Expand在完整返回校验后N786（阶段前757），其它符号/别名N1413/282/1398，原322/337具体与legacyState2358保持。默认限额未升。

首次测试的真实失败保留为记录：新raw测试按实例函数名查找失败，改用实际声明上界识别实例；nested `&&T` 被lexer当作&&，改为合法嵌套类型拼写 `& &T` 后准确触及Expand拒绝。它们是测试入口修正，不改语言行为。

下一公开表示提案：`FunctionLifeContract { binder_count:u32, parameter_lives:Vec<Option<u32>>, result_life:Option<u32> }` 与 `Type::BorrowedFunctionPointer { parameters, result, life }`，canonical dense indices、Wire/fingerprint类型tag31；语法 `for<life a> fn(&'a T)->&'a T`。薄指针可复制，签名资格和实际目标/返回根需独立核；每次间接调用重新代换实际根并核全部窗口。共享patch由root统一发布，未完成消费者前不把表示登记算能力。
