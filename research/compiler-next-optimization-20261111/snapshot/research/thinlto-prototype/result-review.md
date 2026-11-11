# ThinLTO正式两case窗口：非作者原始结果复核

结论：**协议执行完整、功能正确、实际ThinLTO工具链和验证证据成立，但两case运行质量均NoGo。** N64 B/A=1.7357655313，N256=2.0076492560，全部超过固定1.10相对退化门；Rust/B通过0.90门不能豁免。run退出1是完整运行门NoGo，不是编译失败、超时或丢样。原默认配置没有导入任何跨unit body，需与这个性能结论分开说明。

本审只读原始记录、重哈希及纯数学复算，没有编译、执行native、调用LLVM工具或新增计时。复核脚本为`research/thinlto-result-recompute.py`，放在冻结脚本目录之外，未改变protocol脚本集合；所有1044份run文件读前后hash相同。

## 身份与协议是否有效

原始run：`out/next-optimization/thin-runtime-v1/`；冻结输入：`out/next-optimization/thin-protocol-v1/protocol.json`，SHA256 `b5eeba53b6b79b07f825591a6026c3bd568e5a7925fab9744d19a7e603ed514b`。run中的protocol副本逐字节一致，当前脚本与冻结SHA一致，正式source参数、640份ZIL/Rust源文件hash、两case/六排列/总两CPU/所有固定门均重新核对，没有在观察结果后改动。

| arm | 源码与binary绑定 |
| --- | --- |
| A | main `99035931d358add0eaed6d4535035c38413701dd`，tree `1c16744d838064cb1c687ba56ef3a0d17ed3130c`；binary SHA `64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d` |
| B | 研究源码`eb5b150903b67d8b9011f6adc2d3ac67b1c419a6`，tree `1da99d09dbeb53e59f8df4572f4bc5d010ff5929`；binary SHA `aa2b07720075a401e9488ce95d89ff328beee47a58a1f9e63b2625ad57d8b2fd` |
| R | 实际toolchain Rust1.99.1，binary SHA `b53b780f77f952062f8ca8a3b70aef2390b1e45ad91939be94afdc9543026853`；同协议生成的Rust源 |

A/B build binding的source/tree/binary路径及SHA、release成功和build log SHA均核实。clang、ld.lld、opt、bcanalyzer、dis实际版本stdout均23.1.4，所有文件字节匹配protocol pin；lld保留正确argv0，Rust为真实toolchain路径。没有拿通用lld或rustup shim冒充目标工具。编译/native共164份process-attempt均exit0、reaped、无异常/timeout/残留group；manifest为completed NoGo，没有fatal或unfinished记录。因此没有发现使这份有限协议失效的执行问题。

## 编译、当前预检与真实ThinLTO证据

6次clean编译全部正常：每case A/B/R各一次。A走完整LLVM API O2+incremental-o2+after；B同样保留O2前置fold及local/address分组，canonical owner+declarations后交给实际clang/lld O2 ThinLTO；不是旧O0export。两case的A/B完整prepartition `program.ll`原字节也相同，而最终ELF均不同，符合允许不同机器码但必须过真实运行门的研究设计。

| 检查 | 原始证据复核 |
| --- | --- |
| 当前独立语言Verify | A/B共4请求均verification_runs=1，且各有一次verify executed；source closure分别完整64/256文件 |
| A实际unit | 16条builder receipt，全部当前preflight、LLVM23.1.4前后验证、完整default O2/after、x86-64/features空/PIC/default code model |
| B完整machine | 2次原model、2次fold后partition输入、16次实际unit当前validate；外部receipt等于native_codegen中记录，不拿外部LLVM verifier替代语言Verify |
| B实际bitcode | 16份原IR及prelink bitcode保留，实际summary-dump全部含GLOBALVAL_SUMMARY_BLOCK和HASH；target triple正确，保存bitcode的SHA与实际链接输入副本一致 |
| B外部LLVM验证 | 16次原IR verify、16次prelink后verify、16次postlink optimized module verify均有真实exit0记录及对应保存文件 |
| B链接/CPU策略 | 两次真实link的--ld-path指向pinned ld.lld，-flto=thin/-pie、O2/CGO2、x86-64 CPU、ICF none、after merge且aliases false、ThinLTO jobs2/lld threads2；actual工具继承各自ZIL私有group及同两CPU |
| native cache | 每case独立新cache，link前为空，link后的对象及记录hash一致；这是clean质量实验，不是cache命中性能样本 |

### 默认无导入：事实与解释边界

两case各8份实际`.imports`文件，**16份全部为空**；每个unit的保存`.2.internalize.bc`与`.3.import.bc`也逐字节相同（16/16），不仅是某个receipt字段写了空列表。summary/HASH真实存在、actual link及后端优化确已完成，因此不能说“没有启用ThinLTO”或“只是普通原生链接”。实际default policy确实没有选择跨module body import。

原冻结协议要求真实summary/import文件和优化后验证，没有要求非空导入，也没有约定根据观察结果调import预算，因此**空列表不使协议失效，当前默认配置NoGo仍成立**。另一方面，这份实验没有验证到“成功导入所需body后仍保持质量”的状态，不能把失败提升成对所有ThinLTO架构或所有import策略的否定。不能为救本轮结果删除回退、修改阈值或把NoGo改称实现成功；也不能单凭空imports定量分摊全部运行损失。当前输入body的跨unit可见性与baseline不同是事实，具体哪个IPO/pass贡献多少还不是本审证明的结论。

## 54次I/O与独立数学oracle

真实记录为12次额外功能输入（00/ff）、6次warmup、36次formal，共54次；两case各27次，顺序、round/position、arm和输入逐项等于固定计划。每次真实stdin/stdout/stderr、program hash、process-attempt/raw-metrics/record/native-rows之间一致，所有exit0、无stderr，所有输出正确；没有新执行program进行复核。

本审没有调用旧generator的oracle函数，而是直接从四步系数递推重新计算每个seed的函数结果，再按100000=398×251+102聚合。两case全部251个value_by_seed与原独立周期oracle一致，三输入结果如下：

| case | 00输出hex | 41输出hex | ff输出hex |
| --- | --- | --- | --- |
| n64/base | edec0e00 | b3de0e00 | b0e10e00 |
| n256/base | 02e20d00 | 27c90d00 | 14db0d00 |

完整i32输入与中间值边界没有改变；功能正确只能说明本有限输入集符合模型，不能覆盖函数地址、可变global、资源/闭包等未测接口。

## 运行质量重算：不删长尾

全部formal均纳入算术均值，六排列ABR/BRA/RAB/ARB/RBA/BAR不变。wall/CPU单位ms，RSS为KiB。每arm每case仅6点，p95按冻结定义等于max，不能视为稳定尾延迟估计。

| case | arm | mean wall | median wall | max/p95 wall | mean CPU | mean RSS |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| N64 | A | 100.979472 | 100.295829 | 111.191564 | 100.327667 | 312736 |
| N64 | B | 175.276686 | 173.480031 | 191.209908 | 172.865333 | 312736 |
| N64 | R | 404.523314 | 402.332146 | 445.737229 | 399.342000 | 312736 |
| N256 | A | 342.931173 | 337.111702 | 389.752207 | 338.427000 | 312736 |
| N256 | B | 688.485514 | 676.836066 | 741.503428 | 676.942000 | 312736 |
| N256 | R | 1600.932894 | 1597.511033 | 1656.506446 | 1592.241833 | 312736 |

| case | B/A mean wall | R/B mean wall | 固定双门 |
| --- | ---: | ---: | --- |
| N64 | 1.7357655313（+73.5766%） | 2.3079128400 | B/A>1.10，NoGo |
| N256 | 2.0076492560（+100.7649%） | 2.3252964102 | B/A>1.10，NoGo |

所有36个formal原值保留于native-rows和各record，未截尾；本审逐项重算mean/median/p95/max、全部wall/CPU/RSS数组后与summary一致。formal总wall为19878.774316ms、CPU为19680.875ms；额外功能和warmup计时同样保留但按协议不纳入formal均值。RSS恒值有fork/launcher高水位floor，不能推断三个程序live内存相同或更省。总CPU0/1约束不代表锁定CPU钟频，不用环境猜测豁免明显回退。

## 完整编译计时：只报诊断，不宣称净提速

| case | arm | 完整请求wall ms | CPU ms | RSS KiB |
| --- | --- | ---: | ---: | ---: |
| N64 | A | 1361.533355 | 2318.386 | 312736 |
| N64 | B | 6709.575290 | 11814.700 | 312736 |
| N64 | R | 1071.418377 | 1122.643 | 312736 |
| N256 | A | 4350.430653 | 7826.168 | 414744 |
| N256 | B | 9207.138933 | 15753.318 | 312736 |
| N256 | R | 2775.029471 | 4429.576 | 312736 |

B完整请求包含IR输出、wrapper反复工具hash、所有诊断/verify/dump、ThinLTO及native link；这些额外开销属于所测研究wrapper，未减掉。子命令计时与完整请求嵌套，不能相加后扣除。每格仅一次clean编译，协议明确compile_speed_gate=false；不据这组数建立生产编译收益或归因模型。

## 决策和留存

这份**有限两pure base协议有效，运行质量NoGo**；不扩原八case、不改import默认值救样、不进入compile/concurrency矩阵、不采用到生产。它不改变旧1679/a60/结构sink/builder的各自原结论；main990不变。全部正负原始记录保留，没有缺格通过或借Rust门救相对退化。

| 文件 | SHA256 |
| --- | --- |
| protocol.json | b5eeba53b6b79b07f825591a6026c3bd568e5a7925fab9744d19a7e603ed514b |
| native-rows.jsonl | ffc6063d98d7b7947086fa3cb87de3a08d8c04f823b4525ad46857d222ab9161 |
| summary.json | 4abd046f2da5349a9f7deb89106e2301eed336d1b3f498158a29060acdb4784f |
| execution-manifest.json | d04022d31dddfe7b63dc788d8f9153bdf2780d178e26f0d89c0d83400b18f706 |
