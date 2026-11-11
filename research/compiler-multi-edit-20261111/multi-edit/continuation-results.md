# 编译专项续轮结果与归档决策

主线保持`99035931d358add0eaed6d4535035c38413701dd`。本轮没有达到可采纳的新增编译优化：a60投影hash的收益混合且不足以确立整体净收益；1679 bounded imports在真实运行门中8/8失败，已停止扩展测量。main同binary A/A与Rust并发实验及独审已完成：固定四CPU下六格Rust均更快，这只是当前主线诊断，没有候选改善结论。以下均为已有证据的归档，不新增采样、代码或协议变更。

## 1. a60：同机器码，编译收益不足以采纳

候选`a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf`，tree `d260d616a23af8f9c716fbe26c38c200e0eeeb5f`；baseline为上述main。机制只缩小layout action key扫描范围，保留完整O2、当前Verify和完整机器预检，不能以省略检查解释收益。

### 原84请求：诊断结果混合

[逐raw独审：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`measurement-streaming-audit.json`）核实84次当前Verify、456条当前unit预检、42对ELF/materialIR/cache topology一致、252次独立模型输出正确，wall/CPU/RSS全部保留。下表为候选相对baseline的完整wall变化，负值表示候选更快：

| 语料 / jobs | cold | warm | private | edit-warm |
| --- | ---: | ---: | ---: | ---: |
| shared4096 / 2 | +37.47% | +4.84% | −7.60% | −10.51% |
| shared1024 / 2 | −29.20% | −28.48% | −13.18% | −2.89% |
| owned8 / 2 | +0.68% | −3.38% | +1.91% | +6.80% |
| shared4096 / 1 | +10.28% | +16.79% | +8.31% | +4.95% |

这是选定语料的诊断screen，不能据此重算或替代原四频率整体门。[phase归因：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`measurement-streaming-attribution.json`）保留4096 cold的LLVM builder/optimizer/emitter上涨；现有计时缺少layout key专属阶段，无法精确量化新hash省掉的工作。unit elapsed存在重叠，不能从wall扣除，也没有证据把不利结果归咎于环境。原始证据：[streaming-screen：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`original-controls/streaming-screen/`），冻结[plan：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`original-controls/streaming-screen-plan.json`）。

### 新378请求：加权wall −0.947529%，Rust在16个可比CLI格全更快

[独立审计](after-a60-independent-audit.md)核实368条请求行加10次fresh clean oracle，共378次编译请求（ZIL 330、Rust 48），330次当前Verify与2640条当前机器预检；1134次native输出全部符合独立模型。A/B按协议要求与clean oracle的ELF/materialIR一致；这是功能与同质量产物证据，不是native运行性能测量。

N64/256、k1/4/16/64、cold/warm/resident共24格，12格改善、12格回退。按跑前声明的假设频率：N各50%、k各25%、state为10%/45%/45%，mean wall从2691.314387ms变为2665.813402ms，变化−25.500984ms，即**−0.947529%**。这些是协议假设权重，不是用户真实请求分布；CPU频率未锁定，全部长尾保留，小样本p95不作总体尾延迟结论。

Rust在全部16个cold/warm可比CLI格更快，候选/Rust wall比cold为1.377–1.870、warm为1.530–6.250；resident没有Rust等价臂。N256 warm平均仍回退，N64/k4三state均回退；内存也没有确立优势。因此a60不采纳，不宣称全面优于Rust或已建立稳定净收益。完整结果：[run：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`runs/inc-after-streaming-a60/summary.json`）、[rows：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`runs/inc-after-streaming-a60/rows.jsonl`）、[冻结协议：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`final-inc-after-v2/protocol.json`）。

## 2. 1679：真实运行质量门8/8 NoGo

候选`1679decf177c9a6eda5a64063c525f06e44e56a2`仅将既有cost32 bounded source imports推广到无closure/helper的多文件普通程序。运行门先于其编译/并发扩展执行：24次clean编译、216次native（含144次正式计时）全部功能正确，但N64/256 × base/k1/k16/k64八格的B/A mean wall为**1.616–2.123**，全部超过固定1.10相对回退门。Rust/B为2.172–2.327，虽均超过0.90门，不能豁免相对baseline的显著退化。完整wall/CPU/RSS和长尾仍见[运行结果：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`../bounded-imports/runs/runtime-1679/summary.json`）。未展开1679的后续编译或并发矩阵，未调cost32、挑case或按loop回退。

[静态机器码报告](../bounded-imports/attribution-1679/report.md)读取已生成A/B/R产物，没有执行program或新采样。全部8case完整prepartition IR相同；候选根函数的实际callee调用由N64的7→63、N256的31→255，根函数packed SIMD消失；N256主循环还重新保留一次溢出检查。本地叶子向量化仍在，因此证据指向跨unit body可见性损失及其内联/组合优化后果，不能笼统归因为后端换了优化等级，也不能无remarks指定某个LLVM pass的周期贡献。

报告已补历史四臂反证：`8255079de3434de5aad44805a12eed8e9f433c24:changes/research-incremental-o2-quality-closure.md`中的boundedAE/completeAE/boundedInternal/completeInternal，completeAE诊断optimizer更贵且branch质量未全恢复；`b9e566e02cf69b217a2d47c2377340771b02f161`采用Internal还为保住未内联IPO的实际发射。AE不作为本轮新候选，不重跑旧四臂。[历史索引](available-imports-history-audit.md)。现有证据没有建立小而可信的“消除重复优化且保住质量”修复。

## 3. main990：固定四CPU的真实多候选并发

[独立复核](main-concurrency-result-independent-audit.md)已完成。A/B的source均为main990，且指向同一binary路径及SHA256 `64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d`，所以合并四次观测报告current-main，A/A只作波动控制。全部N256，k为每候选修改文件数，c为同时编译的不同候选数；总CPU固定0…3，每任务CPU及ZIL jobs为4/c，缓存/输出目录各自隔离。

| k | c | main批次wall / 秒 | Rust批次wall / 秒 | main任务/秒 | Rust任务/秒 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 1 | 4.400 | 1.330 | 0.227 | 0.752 |
| 16 | 2 | 5.265 | 2.168 | 0.380 | 0.923 |
| 16 | 4 | 6.172 | 3.641 | 0.648 | 1.099 |
| 64 | 1 | 4.873 | 1.337 | 0.205 | 0.748 |
| 64 | 2 | 5.395 | 2.353 | 0.371 | 0.850 |
| 64 | 4 | 7.852 | 4.082 | 0.509 | 0.980 |

wall为所有批次观测的算术均值；吞吐为总完成任务数/总batch wall。c1/2/4每批分别完成1/2/4份不同候选，工作量随c变化，不能把批次wall直接解释为相同工作量的并行加速或退化。增加c提高主线吞吐，但增加每任务latency；Rust对应六格wall与吞吐均优于main。仅一个block，A/A六格漂移为−11.233%至+9.742%，同binary差额不能称机制效果，也不能用于删除a60或1679的不利结果。RSS为逐child高水位，不是并发aggregate peak。

36批共252次编译（84 clean、84 seed、84 final），168次ZIL当前Verify、1344条完整当前unit preflight，756次native I/O全部符合独立模型。56个最终ZIL child的ELF及去唯一fingerprint行的materialIR均与own-binary clean一致；Rust的28个incremental最终ELF全部不同于clean，但冻结协议允许只记录差异，三输入功能核验全部通过，不能写成84个child全部同码。全部wall/CPU/RSS、失败及长尾原样保留；本实验没有生成程序runtime Rust90验收或候选采用结论。证据：[独审JSON：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`main-concurrency-result-independent-audit.json`）、[protocol：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`runs/main-concurrency-990/protocol.json`）、[completion：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`runs/main-concurrency-990/completion.json`）、[batches：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`runs/main-concurrency-990/batches.jsonl`）、[cells：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`runs/main-concurrency-990/cells.json`）。

## 4. 路径反思与分支收束

[N64只读成本诊断](n64-cost-diagnosis.md)已定位真正的重复工作：入口unit含68个canonical定义和504个完整导入body；只改m1一个文件仍使unit1及覆盖全调用闭包的unit0同时失效，根unit重新执行完整O2和机器码发射。a60只复用layout recipe，没有改变这个关键重编范围；1679缩减body可见性则破坏IPO，实际丢失内联、跨调用SIMD及检查消除。AE及CP084已有旧反证，本轮不重复旧NoGo；CP084精确来源为`8255079de3434de5aad44805a12eed8e9f433c24:docs/research/084/RESULTS.md`，其absent-key shortcut功能等价成立，但完整编译收益未建立，不能把“少扫一次图”直接当作实际净提速。当前证据尚未建立保住优化质量且能减少这部分重复工作的可信小修复。

旧`task/compiler-composition-validation`分支已删除，删除前tip `1dfd8b5303041a7fe0e5fbd77fe5efdc8ddd4a56`；研究父提交`5aa8057f9cc0bff651fc6a44147b4e6e2dd307de`保留，归档提交`c43cda662622d81d7a43849ac7bf5266062fc8f7`。[删除与保留收据：原始证据包](https://chatgpt.com/library?file_id=libfile_fde3_f-543f5b7d390481918a9a3fc3e4099f2b_bd147c3485448191a9df16b3c3cea7dd&account_id=personal)（包内`retirement-receipt.json`）记录远端复核，不代表旧候选获准进入生产。**layout-projection和bounded-imports两个新ref仍待归档删除，本文不提前称已删。** main保持`99035931d358add0eaed6d4535035c38413701dd`。

本轮在已恢复LLVM23.1.4/Rust1.99.1的环境中完成，工具全程按冻结身份复用；旧cloud目录不作为本轮复用构建或缓存的来源。完整default O2、独立Verify、当前完整机器预检以及原相对运行门始终保留。

## 证据身份

| 证据 | SHA256 |
| --- | --- |
| a60原84冻结plan | `145d60d2cd88b1e2e9eab9f520aa8949f2e0eeb3bc4a4bc5f25e6f29939c5c07` |
| a60原84逐raw审计 | `424dcd8b401e8f2d6293f9f0ce2ca5554cb69dce03b9fc8e188c2ca395b741a7` |
| a60新378冻结protocol | `dbab00ff9c04bb8a58eaaf0eeba2109f3aefa0d8c2a79bab3638baa49379dd35` |
| a60新378 rows | `e74cbd59411f0add4152e2cccb99a979784011b89e6a90f54e47bf6061fae189` |
| a60新378独审JSON | `9ab44f9962fb21d66a9ed017f81605edadc62d559b7a26c3c809a8cd9868e7f1` |
| 1679运行冻结protocol | `71e918a862acd5f0e7685e6286f906b971fc442fca1d19c2430258069579dee5` |
| 1679机器码报告（含AE历史补充） | `9c17a9825eb06fa579b2a22f1b8f59c96fdfe0f0edb74e5f296aa1f12ccf5348` |
| main并发protocol | `770d5805601c87ed08e220b8ba1aa7ab95684fac32f29bc8524edede7954d25b` |
| main并发独审JSON | `d87f188d8eefc8b6013869e9e116ab18d412ccc14582cc4d79f9241ee1803afc` |

工具身份仍以各冻结identity/protocol为准：LLVM23.1.4、Rust1.99.1及指定clang；本报告不重新绑定工具或修改原始记录。
