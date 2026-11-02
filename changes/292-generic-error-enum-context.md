# 泛型内封闭enum错误构造

基线cd2e42f4。普通generic同步函数返回Fault!i32时，直接
`return Fault.NoMemory`在Expand被误当作enum与整个错误联合类型比较；
相同非泛型返回、普通no_memory()及显式Fault局部变量已经支持。

按设计错误联合既有规则修复：完整实际enum类型恰好匹配E/T中的唯一一侧
才允许注入，联合原值保持；两侧都匹配、名义不匹配、缺失泛型约束仍拒。
不得忽略expected类型或从两侧竞争推导中猜选择，不新增语法/IR/ABI、默认
限额或拥有资格。原源、最近合法对照及独立错误分支机器行为均保留；未调用
泛型定义也须按声明约束检查。必要新遍历照常预算，旧路径费用单独复验。

292施工仅改相关泛型构造上下文及既有功能测试；非作者绑定精确提交审核。
有用源与精简结果入库，不添加工单命名Cargo目标或原始产物归档。

实现片 `c2e5372e`：共享构造上下文仅在一个名义候选时沿用该侧约束；
若两侧是同名enum，必须先由显式类型实参或实际payload建立完整类型，
再按完整类型恰好匹配一侧。不会尝试两侧hint后挑选可用结果。定义检查
与实际实例化使用同一检查；unused声明仍核查。原四份源字节冻结在
`tests/generic-error-enum`，旧资源准备中direct Fault原负例按本片合同晋升。

target-292/jobs1限定结果：Expand generics 9/0、既有enum预算2/0，
driver generics（llvm-api配置）7/0、generic_regressions 9/0；限定Clippy
`-D warnings`及fmt通过。新原源Expand最小steps624/nodes242，N−1
均ResourceLimit，N与fresh相同；旧scalar288/144、owned554/279、
local558/281及enum336/126保持原值。

机器判据直接取源操作：direct Fault、普通helper、显式Fault局部三写法，
各运行成功/错误两分支；成功输出`7`且exit0，错误无输出且exit1。
Clang和LLVM API各O0/O2，共24次运行全部符合（重复检查不计新增覆盖）；
报告backend等于请求，fallback为空。原四份及unique-context独审近邻冻结源均VerifySuccess、
verification_runs=1。无正式全量、公开IR/schema或默认限额变更。

独审近邻补片恢复Engine原本先过滤掉的E!T上下文，仅传给enum检查及透明
括号路径；普通泛型调用仍不从E/T竞争推导。唯一Choice<i32>!i32中的
Choice.None（含括号）现在保持上下文，异实例两候选None仍拒。恢复实例
实际核查后的原源费用为上述624/242；限定9+2+7+9及24机器运行重放通过。
同一唯一Choice实例的If两枝沿相同上下文核查；已specialize的首枝名
不能让第二枝丢失原union上下文。unique-context-if原源也进入Verify控制，
不新增不同E/T混合条件结果规则。上述限定目标及24机器运行再次重放通过。
