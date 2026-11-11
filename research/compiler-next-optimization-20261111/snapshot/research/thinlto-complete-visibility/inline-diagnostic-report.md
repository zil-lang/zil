# N64一次重链inline remarks诊断

## 诊断身份和边界

Root在正式严格两case完成NoGo后授权这一次重链。目录`out/next-optimization/thin-complete-inline-diagnostic-n64-v1/`，plan SHA `11703d7de0fa04226f699d44f134140dc59da83d63a9183dc788063118bed111`。输入恢复自strict正式N64八份真实prelink bitcode，逐份按原receipt SHA核等，复制及重链后再次核源/恢复字节；新输出目录、空新native-cache，CPU固定0,1，保留原ThinLTO jobs2与所有原实际参数，仅追加官方`--opt-remarks-filename PATH --opt-remarks-passes inline`。没有改变阈值、属性、输入、owner、导入范围或预算。

一次实际Clang/lld重链exit0、无timeout，完整stdout/stderr/argv、所有save-temps及attempt落盘；**没有执行产物，没有新增性能样本，不把重链耗时拿来评价编译效率**。中间工具返回的执行耗时不是本报告计时数据。总入口remarks为空，8个backend中只有root backend的`inline-remarks.yaml.thin.1.`非空，其他7份为空。

诊断的unit0 `.4.opt.bc`另外只用llvm-dis解码，与正式strict N64 unit0优化后IR逐行比较：**除第一行ModuleID路径注释外完全相同**，见`formal-shape-comparison.json`。它支持这次remarks解释同一正式热点形状，不声称新ELF等同正式ELF，也没有新运行门。

## 实际LLVM remarks

从LLVM原始YAML按记录边界读取182条，字段保留file/line/Pass/Name/Function/Callee/Caller/Cost/Threshold，结果在`parsed-inline-remarks.json`。分类为168条TooCostly、9条Inlined、5条NoDefinition。这里只取实际序列，不删除重复尝试或不利项。

| 实际调用 | 决策 | LLVM报告cost | threshold | 原remarks位置 |
| --- | --- | ---: | ---: | --- |
| fn14 → root fn2 | TooCostly | 565 | 562 | backend1第1行记录 |
| fn23 → root fn2 | TooCostly | 565 | 562 | 第17行记录 |
| fn32 → root fn2 | TooCostly | 520 | 225 | 第33行记录 |
| fn77 → root fn2（同owner） | Inlined | −14500 | 337 | 第113行记录 |
| root fn2 → 源main fn5 | Inlined | −9675 | 337 | parsed记录保留位置 |
| 源main fn5 → C main | Inlined | −9410 | 225 | parsed记录保留位置 |

root初轮63个leaf调用中7个Inlined、56个TooCostly。7个Inlined正是原unit0的owned callees：fn77/149/221/293/365/437/509，报告cost范围−14510…−14495，threshold均337。56个跨owner AE value都未inline；其中35条报告565/562，其余21条报告cost505…555、threshold225。

root并入fn5、fn5并入main后，56个value在新caller上各再尝试一轮，仍为TooCostly，形成168=56×3条拒绝；fn14/fn23三轮均报告565/562。这与正式strict最终main仍有56个跨owner调用一致。不能只取第一轮或只看最终root名字消失就说全部inline。

## 一个具体的linkage成本机制：sole-call local奖励

官方源码固定LLVM23.1.4 commit `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5`。

- [InlineCost.cpp:1250–1254](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Analysis/InlineCost.cpp#L1250-L1254)的`isSoleCallToLocalFunction`要求callee具有local linkage、恰一个live use且该use是本次直调。
- [InlineCost.cpp:2094–2114、2193–2198](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Analysis/InlineCost.cpp#L2193-L2198)从TTI取得LastCallToStaticBonus，并在该条件满足时从cost减去它。
- [TargetTransformInfoImpl.h:98–101](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/include/llvm/Analysis/TargetTransformInfoImpl.h#L98-L101)默认返回15000；同版本X86TargetTransformInfo.h/.cpp未override该hook。本轮目标x86-64，参数未改。

实际`.3.import`里，fn77为Internal，只有root一处直调，body177条；fn14为AE，也只有root一处直调，body178条。两者大小几乎相同，而local sole-call满足与否有巨大cost奖励差异，且remarks实际观察到fn77约−14500与AE正cost。它是比“AE不能做IPO”更具体的**内联成本模型差异**，不依赖把显式range分析错误地说成失败。

这是当前严格模块中的局部对照，不是已执行的新private-import候选。没有把原AE改Internal、没有附加alwaysinline、没有把阈值加大，也没有证明“只改这一点就能恢复全部运行质量或降低编译成本”。将来任何实现还须保持canonical地址、direct-only改写、完整原copy预算及真实质量门；本报告不授权或实施新方案。

## 阶段顺序的证据与限度

此前只读数据已证fn14从45条且8个helper调用变成prelink178条、helper调用0，fn23从45变156；其真实import body与prelink body逐字节相同。这个CGU prelink膨胀会改变后续inliner输入，仍是相关机制。

但“更大所以不inline”不够解释全部：同owner fn77也为177条却因local sole-call奖励显著胜出。严格AE仍带返回range[-2000,2001)，call输入有range[-250,251)，root累加overflow已大量消除；**显式范围传播成功与未inline同时发生**。不能把结果归结为范围属性没生效，也不能把15000奖励与所有非inline IPO权限混成一个说法。

Blaze逐LLVM函数prelink没有本CGU内八个helper定义，所以不会在该LLVM阶段以同样方式展开；但其MIR inliner可预先把callee built_in进own，postlink仍有自有导入与inline过程。它不证明逐函数prelink必能恢复ZIL运行质量；尤其即使保留45条带helper调用的value，后续CGSCC仍可能先处理helpers再处理value。本次没有做该新变体，不能越过原NoGo或用另一上游结果代替本项目运行门。

## 不能把565/562解读为“只差3，微调即可”

这是LLVM当次报告的cost，不是源码或机器指令数。[InlineCost.cpp:1139–1147](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Analysis/InlineCost.cpp#L1139-L1147)允许在超过threshold时提前停止、低估完整cost；ComputeFullInlineCost/具体ORE开关控制是否走完整分析，见1215–1217。序列化remarks不应被未经检查地等同强制完整cost分析；[InlineAdvisor.cpp:169–175](https://github.com/llvm/llvm-project/blob/7a941dfb2877a0b8cf30323fefa85ad019e0b0f5/llvm/lib/Analysis/InlineAdvisor.cpp#L169-L175)按missed-remark diagnostic开关决定传入ORE。

本次没有追加inline-cost-full、改阈值或再链一轮，故保守称“报告565，大于当时562”，不称“完整cost只超3”。不能据此建议阈值扫描或声称改到566会成功。新证据所支持的是：完整导入已排除缺body，实际拒绝为TooCostly，local sole-call成本奖励提供了可定位的linkage差别；其他因素的独立贡献尚未隔离。

## 保全

`inline-diagnostic-evidence.json`列出plan/attempt/raw remarks/parsed records/shape comparison的SHA与字节数；raw目录完整保留重链save-temps、stderr及新cache。stderr包含已知getchar无程序内body的force-import诊断，未删除，实际重链exit0。没有原生运行、没有扩大正式两case、没有覆盖或补跑原结果。本轮归档补充已按Root要求暂停，新的诊断材料没有擅自复制进旧归档快照，由Root最终归档。
