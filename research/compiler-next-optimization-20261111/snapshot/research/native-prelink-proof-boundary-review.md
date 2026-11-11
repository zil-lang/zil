# Native prelink：typed proof与实际闭包的最小边界

## 结论

按当前职责拆分足够：backend沿同一次当前Verified/lower/fold/原Complete planner及完整机器预检产生`CheckedProof`，native根据同请求opaque `PrelinkedUnit`的真实LLVM事实重新求实际Required并检查其属于Allowed。**无需native第三遍重算原sourcecost和Complete预算，更不能从JSON receipt恢复许可。** 本审只读当前未冻结源码及设计，不改代码、构建或运行LLVM/native；最终API尚由作者实现，下面的安全条件不是已经执行通过的结论。

## 最小bundle及来源

现实现的`CheckedProof { receipt, units: Vec<CheckedUnit { canonical, allowed }> }`已经够用：

- `Proof::prepare`持有完整当前source facts、原cost/调用图、foreign和原Complete结果；`finish_checked`沿旧finish算法核唯一owner双射、原source闭包恰等Allowed、checked-add总copycost不超sourcecost，成功后才产生typed集合。旧`finish`委托同算法取receipt，原错误优先级与silent-budget-fallback拒绝不另写一套。
- cost、owner/canonical证明留在这个生产链及输出receipt即可；跨crate只传canonical集合和目的Allowed名字，不需要再传JSON布尔“已验证”。typed结构是当前内部事实，不是新的ZIL VerifiedModule能力。隐藏native API仍只能处理可信编译器产生的模型。
- backend prelink每个当前unit，并核`facts.unit==id`、`facts.canonical`唯一且精确等同当前checked canonical。仅owned字节/事实跨阶段屏障，不共享LLVM句柄。opaque `PrelinkedUnit`必须由真实preflight/build/LLVMverify/prelink/verify成功构造，不能公开任意BC+claims构造器。
- native负责实际definitions/declarations/direct calls与每个body唯一owner；这是补足LLVM优化后事实的独立复核，不是再次发明原planner权限。

已读当前`private-imports/.../build/research_prelink.rs`：八unit/无cache检查、两phase join、同jobs、按unit确定性收错、取消检查、只写staging object再走既有最终link/publication均成立。原build研究门拒closed/cache/旧exporter混用、要求Linux x64/API/O2/incremental-after/8unit2jobs和fresh绝对receipt。普通入口仍走旧compile_units，旧pipeline不叠新分支。

## 实际prelink闭包与切片

Required必须从目的unit的**全部当前own prelink definitions**出发，在全体真实prelink调用图上visited闭包；每个非foreign target有唯一body，跨owner目标进Required，最终`Required ⊆ Allowed`。原finish已证Allowed等原始完整Required，所以这就是允许LLVM简化后缩小需求而不扩原许可。不能只数首层callee、仅从main出发、按size筛掉body，或把缺失定义当foreign。

selected按source owner分组。先证全闭包，再将donor非selected定义internalize并GlobalDCE，之后要求actual definitions**精确等于selected**、LLVMverify；不能用DCE结果反过来扩大Allowed。每次link后实际definitions应为own加已选Required，拒新名字/重复owner/自动后缀；所有source在同一目的Context解析，LLVMLinkModules2无论成功失败消费源Module，失败即整次拒绝。最终全量only-direct检查通过后再一次私有rename，不能边检查边改。

递归与SCC用visited去重、不按当前use数量过早裁剪。反例：目的f导入g，g调用同donor h；h必须被实际闭包纳入，否则DCE保留h后的集合门拒绝。g↔h互递归不能仅留g；g→目的own f应继续指向canonical own，而不是额外复制f。普通canonical owner是保留根，不能被为了donor切片而顺手Internal化/删除；其他unit规范地址不因私有实现改名改变。

## 类型、attrs和only-direct

model preflight覆盖当前调用签名与其本unit声明，但native linking仍需保持真实函数类型、return/parameter/function attrs、calling convention及target/layout。opaque pointer下`LLVMTypeOf(Function)==ptr`不足以证明类型一致；实际CallInst应核`LLVMGetCalledFunctionType`与目标`LLVMGlobalGetValueType`相符或等强度检查。NamedType字符串相同也不证明不同module里的定义布局相同，应由当前同源模型/真实link与verify边界闭合，不能仅比较名称就宣称ABI相同。

不要把旧caller declaration attrs覆盖已prelink definition，也不能把caller特定参数事实泛化为canonical exporter attrs。复用的是同一prelink产出的body及其合法优化attrs，own/import保持来源一致；private rename只改名/linkage及setter隐含header，不加alwaysinline、nonnull/noundef/range等假设。

逐Use必须要求真实CallInst、无bundles、operand数等args+1、当前Use恰是callee operand、called value就是该Function，且类型一致。同User合法`call @f(ptr @f)`、ConstantExpr/Array/Gep地址、metadata value/global/alias/ifunc、间接调用及未知function先拒，不能strip cast后当直调。metadata递归应拒Value逃逸而非完全忽略metadata；固定pure范围外不扩大。

## 最近反例与验收边界

优先复用现有原planner silentfallback/预算N−1/owner错配/递归控制，native新增边界只需覆盖：实际Required越Allowed或缺body、donor未选可达h导致DCE集合不等、互递归闭包、同名不同函数类型、合法callee兼参数地址Use及link失败所有权。它们是新增实现边界，不要求另造完整预算重算器或扩大benchmark。

尚待精确API源码核验实际type/use检查、bitcode/Linker所有权、DCE根、module flags及verify checkpoint；本报告不代报这些实现已通过。质量可能因没有真实summary/index、prelink/链接顺序及inliner权限而改变，必须重新用固定完整请求与原1.10/.90门取得证据，旧private运行Pass不能自动沿用。无cache首版也不提供many-edit命中结论。
