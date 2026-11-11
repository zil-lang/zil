# 结构化sink 84请求非作者原始结果复核

结论：**原始证据完整、功能和同码控制通过；编译净收益未建立，本候选不采用，不扩矩阵。** 本审只读现有raw并复算，没有运行编译器、native或新性能采样；没有从summary复制统计、删除不利样本或归咎环境。源码静态无阻断不等于性能合格。

## 身份与方法

原始run：`out/next-optimization/structured-screen-ee0/`。baseline源码`99035931d358add0eaed6d4535035c38413701dd`，被测候选`ee0cc45261dbfadf25dce567f0fc1fa0b4d4b6f5`的tree为`ad37913cf49a1bfec28e66e7abdcf225dbcafed5`，与已审`924076ca11c75e88bb1bcb9df98c58d0dae1bbd2`源码树一致。主线/候选binary SHA256分别为`64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d`和`adf69c444bb689e961f7dc404d75e6da0f7aec1e597b3f557919cdaf0bce8043`，实际文件与plan均重新重哈希。

复核工具为本目录`recompute-result.py`：读取84条rows、42条pairs及每请求实际record/stdout、ELF/sidecar、native stdin/stdout/stderr，逐项与冻结plan匹配。另用独立写出的共享leaf递推/拥有状态闭包整数模型复算18个case/snapshot/input oracle，全部与plan及252个原始输出一致，未调用原生成器的expected函数。所有被审日志和顶层文件共1183份在读前/读后SHA256相同。源语料、runner/model和工具hash也与plan一致。

## 当前Verify、预检与产物

| 项目 | 从raw核实 |
| --- | --- |
| 实际请求与完整顺序 | 84/84，42对，与plan每个arm顺序、case/jobs/request/snapshot/repetition一致；每条record等于对应rows记录 |
| 功能/失败 | 全部exit0、无timeout、无errors、Success；failures.json为空；没有invalid行过滤后再宣布Pass |
| 独立语言Verify | 84次verification_runs=1，且每个stdout均有且仅有一次verify executed事件 |
| 当前机器预检 | 456个实际unit receipt，全部preflight_ns>0；每个请求receipt数量等于actual_units |
| LLVM builder | 54个，全部LLVM23.1.4、完整`default<O2>|merge-functions=after|aliases=false`且verify_before_after=true |
| 对象命中 | 402个，全部仍有当前完整preflight；llvm_verification_this_request=false明确保留，producer_verified_before_after=true；没有把旧producer核验冒充当前LLVM builder核验 |
| 其他路径 | object-patch为0；每对actual_units/jobs/fallback及cache hits/misses/corrupt/stored/write_failures一致，逐unit input_kind和object_bytes序列也相同 |
| A/B产物 | 42对完整ELF原字节相等；84份日志ELF及sidecar的实际hash匹配各自记录 |
| material IR | 42对仅删第二行唯一verified-fingerprint后的sidecar原字节相等；raw sidecar 0/42相等，差异没有被隐藏为raw同码 |
| native I/O | 252份真实stdin/stdout/stderr全部核验，三输入符合独立整数模型，退出码记录全部0；此为功能控制，不是重复runtime计时 |

## 完整wall、CPU、RSS复算

每arm42请求的固定screen工作总量：wall由**34171.077129ms→36779.215848ms（+7.632591%）**，进程CPU总和由40163.857ms→43266.018ms（+7.723763%）。这是已声明42个请求的工作总量，**不是用户发生频率加权score**，没有借用原八语料/四频率整体结论。

下表每项均为该格全部观测算术均值。wall/CPU单位ms，RSS单位KiB；负变化表示候选更快。cold/private每格只有1点，warm为7点（jobs1为3点），edit-warm为3点（jobs1为1点）；不给这些小样本宣称稳定p95或普遍因果。

| 语料 | jobs | 请求 | 每arm n | A wall | B wall | B/A变化 | A CPU | B CPU | A RSS | B RSS |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| shared4096 | 2 | cold | 1 | 5386.236 | 5758.249 | +6.907% | 9316.855 | 9798.640 | 529168.0 | 562004.0 |
| shared4096 | 2 | warm | 7 | 537.573 | 523.729 | −2.575% | 649.478 | 641.358 | 312676.0 | 312676.0 |
| shared4096 | 2 | private | 1 | 4308.777 | 4782.752 | +11.000% | 4478.895 | 5115.527 | 521044.0 | 498088.0 |
| shared4096 | 2 | edit-warm | 3 | 533.482 | 645.237 | +20.948% | 635.748 | 818.745 | 312676.0 | 312676.0 |
| shared1024 | 2 | cold | 1 | 1048.809 | 862.677 | −17.747% | 1644.265 | 1377.787 | 312676.0 | 312676.0 |
| shared1024 | 2 | warm | 7 | 178.066 | 169.775 | −4.656% | 202.757 | 200.614 | 312676.0 | 312676.0 |
| shared1024 | 2 | private | 1 | 775.991 | 783.491 | +0.966% | 811.949 | 809.810 | 312676.0 | 312676.0 |
| shared1024 | 2 | edit-warm | 3 | 143.491 | 151.104 | +5.306% | 167.773 | 179.343 | 312676.0 | 312676.0 |
| owned8 | 2 | cold | 1 | 108.393 | 113.237 | +4.468% | 108.999 | 113.541 | 312676.0 | 312676.0 |
| owned8 | 2 | warm | 7 | 35.245 | 43.285 | +22.813% | 34.453 | 41.740 | 312676.0 | 312676.0 |
| owned8 | 2 | private | 1 | 105.004 | 107.662 | +2.531% | 104.202 | 106.880 | 312676.0 | 312676.0 |
| owned8 | 2 | edit-warm | 3 | 34.373 | 36.089 | +4.992% | 33.365 | 35.495 | 312676.0 | 312676.0 |
| shared4096 | 1 | cold | 1 | 8437.757 | 9792.552 | +16.056% | 8412.733 | 9760.047 | 521600.0 | 514696.0 |
| shared4096 | 1 | warm | 3 | 628.488 | 573.800 | −8.701% | 620.308 | 571.346 | 312676.0 | 312676.0 |
| shared4096 | 1 | private | 1 | 4174.572 | 4629.257 | +10.892% | 4161.394 | 4614.017 | 500676.0 | 496944.0 |
| shared4096 | 1 | edit-warm | 1 | 549.853 | 573.127 | +4.233% | 546.169 | 568.997 | 312676.0 | 312676.0 |

16格仅4格wall改善、12格回退；全部行和长尾保留。A RSS观测范围312676–529168KiB，B为312676–562004KiB，没有整体内存优势；ru_maxrss是父进程/已回收子进程OS口径的单进程高水位，不是同时live总内存，且存在launcher继承floor，不能从重复恒值推断实际内存完全相同。固定CPU0/1不等于CPU钟频固定。现有数据不定位唯一噪声/阶段原因，不从wall扣除LLVM时长，也不以“只少一个event buffer”为不利结果豁免。

## 决策边界与证据hash

按这份有限screen停止本候选：不采纳、不加轮次、不展开更大矩阵、不调整权重或删格。没有Rust编译臂，没有生成程序重复runtime Rust90门，也没有新的总体performance成功主张。main990及其他旧NoGo身份保持不变。

| 证据 | SHA256 |
| --- | --- |
| plan.json | `6fb22be8b6c199e40cb7f6a2370f6ea7b9055c0f02d81ad7a92cf48acda5ff2c` |
| rows.jsonl | `de7e2638cb2b36e97f8f18476cb0000cca2e232a90a19746f5aaac1f96c3aa5c` |
| pairs.jsonl | `1040157caca23c4400103093b49ae88dcf50898ab8e81f14ee00f52b78ac9f21` |
| failures.json | `37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570` |
| completion.json | `250618c99ca69f4d77cad2f68027e7aac2d108d330817831f9653a0968dfcf49` |
