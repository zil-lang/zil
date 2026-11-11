# main990与Rust：固定四CPU并发实测独立复核

结论：**36批数据完整、协议及功能/同码控制通过；本结果测量当前main的并发能力，没有任何优化候选改善结论。** A/B都指向同一份main990编译器，相同path与SHA256，故合并四次ZIL观测报告current-main，A/A差异仅作为波动控制。1679仍因自身runtime失败而NoGo，其编译/候选并发没有执行。

本独审只读原始记录并重算，没有调用编译器、原生程序或新增采样。完整脚本为recompute_main_concurrency_990.py，逐批全部值、CPU/RSS及计算结果为main-concurrency-result-independent-audit.json。

## 身份、计数与实际控制

| 项目 | 独立核验 |
|---|---|
| 协议SHA256 | `770d5805601c87ed08e220b8ba1aa7ab95684fac32f29bc8524edede7954d25b` |
| batches.jsonl SHA256 | `52b135dcb9d848f3c57d64773cc3ac0af8866543e7dbe7f76438da33bf3c67ff` |
| completion.json SHA256 | `81359877221e921e1feb30d81739676d49a8507a015b4e5d203b5b6ef2690bbd` |
| A/B source | 均`99035931d358add0eaed6d4535035c38413701dd` |
| A/B共同二进制SHA256 | `64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d` |
| 真实输出目录 | 协议out指向`runs/main-concurrency-990`，运行协议副本逐字同哈希 |
| 最终批次 / children | 36 / 84，ARBBRA位置与全部冻结shape逐项相同 |
| clean / seed / final编译 | 84 / 84 / 84，共252：168 ZIL、84 Rust |
| 当前ZIL Verify / unit preflight | 168次均Verify1；1344个当前完整unit preflight |
| unit类型 | 1036 builder带当前LLVM前后核验；308 object-cache仍当前完整预检；patch为0 |
| native I/O记录 | 756份，全部符合独立三输入oracle |
| 完整batch-attempt | 204份：36 final+84 clean+84 seed，均无异常/unfinished/强制清理 |

固定脚本/helpers、A/B/R/clang实际字节及全部fixture哈希均重新计算。每个最终child的source目录与其他child隔离，实际最终源文件全部重哈希；同批candidate1…c的salt快照不同。compiler raw stdout中的实际N256、LLVM23.1.4、完整O2+after、8CGU、jobs=4/c和--clang均与协议匹配，Rust为相同slice的正常incremental构建。

56个最终ZIL child的完整ELF和仅去唯一diagnostic fingerprint行的material sidecar，全部匹配同二进制clean；旧执行器B lane使用A clean，但两者本就同一文件/同字节，因而确为own-binary clean。另对相同salt的56组跨lane配对比较，ELF/materialIR也全部一致。

Rust的28个最终incremental ELF **全部不同于其无incremental clean ELF**，原冻结协议明确只记录此差异、不强迫相等；独立三输入功能核验全部通过。不能笼统写“84child全部同码”，也没有把这些Rust差异隐藏为true。

所有seed/final/clean原始CPU/latency/exit与batch-attempt一致，raw先于post-validation保留；原cells.json的wall/CPU/每child latency/RSS数组全部独立核对。原生I/O由旧执行器capture后保存在JSON，协议没有为每次native另存stdin/stdout文件；本审核核对这些全部原始捕获记录与独立整数模型，没有声称读取不存在的原生独立日志。seed program/source已被final覆盖，其原记录可核，过去的文件不能重新哈希，此限制如实保留。

## 总四CPU下的批次wall和A/A波动

全部N=256；k是每个候选变化文件数，c是同时编译的候选数。A/B各两批；current-main合并同二进制四批；Rust两批。单位秒，全部观测算术均值。

| k | c | 每任务CPU / ZIL jobs | A批次wall | B批次wall | current-main批次wall | Rust批次wall | A/A的B相对A漂移 |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 4 / 4 | 4.322 | 4.479 | 4.400 | 1.330 | +3.627% |
| 16 | 2 | 2 / 2 | 5.440 | 5.090 | 5.265 | 2.168 | −6.440% |
| 16 | 4 | 1 / 1 | 6.539 | 5.804 | 6.172 | 3.641 | −11.233% |
| 64 | 1 | 4 / 4 | 4.977 | 4.768 | 4.873 | 1.337 | −4.194% |
| 64 | 2 | 2 / 2 | 5.491 | 5.299 | 5.395 | 2.353 | −3.482% |
| 64 | 4 | 1 / 1 | 7.488 | 8.217 | 7.852 | 4.082 | +9.742% |

A/A六格漂移−11.233%到+9.742%，源码与生成代码相同，故不能把某一lane较快解释为优化。只有一个block，主线四批和Rust两批不足以给这些差额稳定因果解释；所有值保留，没有追加轮数或筛选。

## 每任务latency、throughput和CPU

latency为单个child启动到wait4回收的均值；完整batch-start到各child结束的另一口径与所有个体值保留在JSON。CPU为每批各child wait4 user+system总和，包含OS记账的已回收后代；单位秒。throughput按“全部完成任务数 / 所有批次wall之和”计算，未平均每次倒数。

| k | c | main每任务latency | Rust每任务latency | main任务/秒 | Rust任务/秒 | main每批CPU总和 | Rust每批CPU总和 |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 16 | 1 | 4.400 | 1.330 | 0.227 | 0.752 | 5.006 | 3.356 |
| 16 | 2 | 5.227 | 2.152 | 0.380 | 0.923 | 11.819 | 6.848 |
| 16 | 4 | 5.980 | 3.549 | 0.648 | 1.099 | 23.867 | 14.159 |
| 64 | 1 | 4.872 | 1.337 | 0.205 | 0.748 | 6.100 | 3.641 |
| 64 | 2 | 5.366 | 2.281 | 0.371 | 0.850 | 12.764 | 7.300 |
| 64 | 4 | 7.547 | 3.919 | 0.509 | 0.980 | 30.114 | 15.625 |

在这组固定四CPU观测中，c增加提高current-main吞吐，同时提高每任务latency：k16从0.227升至0.648任务/秒，但每任务4.400→5.980秒；k64从0.205升至0.509任务/秒，但每任务4.872→7.547秒。Rust同样存在吞吐/latency取舍，且各对应格wall与吞吐均优于current-main。

c1、c2、c4每批分别完成1、2、4份不同候选；不能把6.172秒与4.400秒直接当作“同工作量并行变慢”，也不能称为使用更多CPU取得额外收益。总CPU始终0..3四个，slice不重叠；随着c变化的是每任务CPU和内部jobs。语义缓存、对象缓存及Rust incremental都在各任务独立source/cache上先seed再final，本协议不等同于此前双CPU单请求warm矩阵或Cargo全项目构建。

## RSS与结论边界

最终child的wait4单进程RSS观测范围194916–421696KiB，完整逐child数组保留。它不是同一时刻的并发aggregate peak，不能把c个ru_maxrss相加当作峰值；fork/launcher已发生的高水位还可能形成观测floor，不能由恒值或近值推断实际live内存相同。没有额外内存试验或改旧协议。

本测量完成的是当前main对真实多文件候选的c1/2/4并发诊断；没有新编译器实现、没有候选采用、没有原发生频率综合score，也没有生成程序runtime Rust90验收。1679的NoGo独立保留，a60的未采用结论也未改变。
