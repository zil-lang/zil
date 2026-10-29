# 213–217：真实主线合流、Product拥有捕获与ZUI语言起步验收

日期：2026-10-29。内部schema/Wire仍为27。完整语言NotSatisfied，B0/181继续施工，产品/原生WindowsNotRun。当前报告只对明确的有限范围作Pass裁决。

## 本轮合流与施工

实际恢复的语言主线已经到206（8527fb39），并非止于202；202及旧203已在其祖先中。213将其与GitHub a0796270中的LLVM212直接Builder、当前Verify后的对象缓存及研发API默认合流。真实共同祖先为1d7216f0，102个冲突文件按原父链解决；前端采用语言206语义，后端把新增资源/Store/Product清理迁入结构化machine events，没有重写Git父链或接入未准入的136优化。

214–216增加完整有限嵌套Product的拥有闭包捕获。显式move只读Fn、实际消费的FnOnce、每个普通/错误出口回填的FnMut均有真实源码与机器证据。完整child在环境中有独立责任slot/flag，先清标志再按声明序递归清理；实际叶来源由Analyze与Verify各自核验，捕获不制造Created。能力通过factory、State和外层callback Store组合，保持内联布局、默认限额及独立Verify。

217新增五模块普通ZIL消费者，实际组合Product FnMut、ByteBuffer FnOnce、双环境Node<F,G>、Store、ID关系、立即删除、兄弟继续调用、替换和错误清理。输入改变轮次/删除/更新/退出；初始3节点，第4节点走满容拒绝，删除采用O(n)后缀转移。它是语言组合验收，不是ZUI框架交付、任意规模图或性能证明。

## 实际验证身份与结果

| 验证 | 实际受测源 | 结果 |
|---|---|---|
| 213默认完整workspace | f581cd67 | 2702/0/0 |
| 最终默认完整workspace | 8628d976 | 2743/0/0，285组 |
| 最终参考后端完整workspace | 46a2947f | 2711/0/0，生产/测试与8628相同 |
| fmt、all-target Clippy、release CLI | 8628d976 | Pass，Clippy -D warnings |
| 既有208 API消费者 | 8628d976 release CLI | 16构建、128执行、56实际分配失败，Pass |
| 213独审 | f581cd67 | 134限定测试，16构建、444执行、260实际分配失败 |
| 214–216独审 | 68dd99af | 101限定测试，72构建、108执行、24实际分配失败 |
| 217作者及非作者分别复跑 | 消费者06a80559、编译器68dd99af | 每方116执行/74实际分配失败，另2索引控制 |
| 217非作者追加API原对象/缓存 | 同上 | 4构建、204执行、148实际分配失败 |

这些计数按来源分列，不求和为独立用例总数。所有分配探针最终live0；217布局与峰值在执行前冻结oracle，非作者另手推Node112B控制。原对象API故障探针只在链接阶段增加wrapper，默认/冷API使用Builder且parse_ns=0，缓存命中仍运行当前Verify与machine preflight。新Types/Analyze/Verify工作边界分别1006/3254/3405，均核N/N−1/fresh；旧flat控制保持，默认限额未调整。

## 保存的失败与迁移

原206源29及协调raw Product捕获的合法形状，在新合同下明确晋升接受；原源码字节与真实旧拒绝日志保留。另一旧206恶构造仍拒绝，仅将捕获资格Unsupported迁为实际字段核的精确SafeReject。未删除邻近反例、未放宽泛型边界或默认预算。conditional Drop有意幂等，重复Take仍拒绝；最初夹具中错误调用模式造成的提前拒绝已拆开核验。两次磁盘耗尽归类InfraFail；原全量断言Fail、夹具Fail和重跑身份均保留。

867/213更改规范时曾漏同步规范登记，本轮只修当前七份输入登记与Product有限状态说明；旧验收、旧登记和作者交付SHA没有重写。最终补充独审按逐文件字节等价衔接f581/68dd/06a与最新源，不把旧执行改标为新执行。

## ZUI起步范围与仍未实现项

普通同步State、Store、固定F/G家族、ID节点关系、同步借参和显式平台循环已有可运行路线。嵌套Store、拥有Sum和Product自身成为Store元素虽仍不支持，但不应仅凭其Unsupported就断言所有同步框架无法起步；本消费者给出具体替代表达。

仍须保留的真缺口包括默认只读Product借入、更多递归拥有组合、拥有Sum、一般高阶参数/结果、trait关联项/life/HRTB/dyn、声明式ui!/卫生Type–Syntax生成，以及长期外部userdata、Active重入、挂起/取消/并发。它们对于相应API是实际前置，不能借本轮有限同步验收宣称全部完成。详见217矩阵，每项区分旧证据、本轮执行、替代表达与未实现边界。

## Rust四目标与本轮突破

安全：独立Analyze/Verify分别核实际位置、child责任、完整出口和incoming来源，并拒绝同型捕获伪造。运行/内存：没有新增隐式Box/RC/锁/全局表；清理与真实分配失败有证据，但无Rust性能比较。表达/维护：嵌套拥有状态现在可随保存回调组合，跨模块消费者不借助新权限。编译成本：精确预算和旧控制保真，不宣称编译提速。实质推进是Product拥有环境从普通值走通到State/callback Store的完整纵向链；完整语言和产品条件不因此升级。

## 同步和分支

v5已有-AllowMain选项，但默认跳过main；v6默认main同名同步，新增-MainOnly及-SkipMain，保留自动发现包、ZIP、逐分支改名、Enter不改与成功回读。-MainOnly先证明bundle内其他head都是main祖先，分叉不覆盖。18组真实PowerShell/Git端到端通过；原生Windows PowerShell5.1未执行。

远端发布使用单次受测bundle，精确head快照、main祖先检查、逐ref lease和原子更新。先保留归档标签，再退役202/203和旧132/135/136分支；131只收纯研究，不计生产优化。具体远端结果另写发布收据，不能仅凭本地提交声称已推送。
