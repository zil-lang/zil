# 1679真实运行质量结果：独立逐raw复核

结论：**NoGo，1679不采用；其后续编译收益矩阵和候选并发screen保持未执行。** 本轮24次clean编译、216次原生运行完整执行且输出全部正确，但8个case全部违反B/A<=1.10：候选相对main运行wall增加61.619%–112.321%。Rust/B>=.90虽然8格都通过，也不能抵消独立的相对main退化门。

这是完整性能门失败，不是解析、编译、功能、timeout或测量程序异常。本独审仅读已有raw/源码身份并复算，没有运行program、编译器、测试或新增采样；独立计算在有空余CPU时避开CPU0..3，不占root测量窗口。

## 精确身份

| 项目 | SHA256 / commit |
|---|---|
| runtime协议 | `71e918a862acd5f0e7685e6286f906b971fc442fca1d19c2430258069579dee5` |
| native-rows.jsonl | `46905333582035f34547a61029b513beb7001269b9848299e2b533e4e5efc354` |
| summary.json | `036b26dc6f4fb37d4149afec10fd07744b8d60c3c69a60d53fde2d0acc61389e` |
| 基线source | `99035931d358add0eaed6d4535035c38413701dd` |
| 候选source | `1679decf177c9a6eda5a64063c525f06e44e56a2` |
| 基线编译器字节 | `64e1ff43bbaa59f786f1cc899b69e707072219a9c0abf0122ce31cd7b77f981d` |
| 候选编译器字节 | `04d8af53887b0a159684eedb03a0763ac45c19ffabacdf1e599de94531086339` |
| Rust编译器字节 | `b53b780f77f952062f8ca8a3b70aef2390b1e45ad91939be94afdc9543026853` |
| clang字节 | `c336d48383874cffaab0dc4c5c359456d871f21380b746520dde5c7f4c125a67` |

以上实际文件、runtime-frozen-1679五个脚本、reference协议和全部runtime源哈希都重新计算并与协议匹配；Rust原始版本文件要求1.99.1。A/B均显式使用同一固定clang，完整O2/incremental-o2/after、8CGU/2jobs，无object/semantic缓存；Rust O2/overflow checks/panic abort/8CGU和同clang linker均实核命令。

## 逐raw与完整计数

独立审核读取24份编译record/raw-metrics/process-attempt、ZIL原始stdout；读取216份原生record/raw-metrics/process-attempt及实际stdin/stdout/stderr文件。每份聚合row与其原record一致、metrics在三个载体之间一致，实际输入一字节、exit0、stderr空、精确输出字节均核对。24份ELF与16份ZIL sidecar当前文件重新哈希，运行row的artifact身份对应到实际program。

| 控制 | 独立结果 |
|---|---:|
| clean编译 | 24：16 ZIL、8 Rust |
| 当前语言Verify | 16/16均执行一次 |
| 当前完整machine preflight | 128/128 actual units |
| 当前LLVM前后核验 | 128/128 builder units |
| 原生正式运行 | 144：8case×3arm×6 |
| 预热 / 额外功能运行 | 24 / 48 |
| 原生总数与实际I/O核验 | 216/216 |
| 非零exit / timeout / exception / cleanup | 0 / 0 / 0 / 0 |
| 丢弃outlier或失败 | 0 |

实际native顺序与8case的完整冻结schedule逐条相同，无缺格、重复或替换：先00/ff额外功能，再41预热，最后ABR/BRA/RAB/ARB/RBA/BAR六排列。每arm每位置恰两次；所有六个正式wall/CPU/RSS观测重新计算mean/median/p95/max，与summary逐字段精确一致。manifest为completed=true/status=Failed，没有fatal收据；代表完整运行后明确失败。

## Oracle独立复算

对每case从N、k重新计算selected文件，再按整数规则独立求所有251个seed的value，核对冻结value_by_seed与cycle_sum。另用逐次100000步的acc=(acc+value[(input+i)%251])%1000003循环复算三个input，没有复用runner的“398整周期+102尾项”汇总公式；与所有原始输出一致。程序值域保持seed0..250、leaf乘加<=3039、N256 acc+value<=1510005、input+i<=100254，不存在i32溢出造成的不公平语义。

## 每case真实结果

以下为全部六个正式样本的算术均值，单位ms。Rust/B列是运行wall比值，越大表示候选相对Rust越快；它与B/A门分别裁决。

| case | A wall | B wall | Rust wall | B/A | 退化 | Rust/B | A CPU→B CPU |
|---|---:|---:|---:|---:|---:|---:|---|
| N64 base | 103.710 | 169.756 | 383.997 | 1.636829 | +63.683% | 2.262061 | 99.861→169.244 |
| N64 k1 | 98.534 | 171.951 | 387.658 | 1.745083 | +74.508% | 2.254470 | 98.111→170.768 |
| N64 k16 | 98.032 | 172.502 | 374.650 | 1.759651 | +75.965% | 2.171861 | 97.754→168.605 |
| N64 k64 | 110.290 | 178.249 | 398.659 | 1.616187 | +61.619% | 2.236533 | 110.020→177.588 |
| N256 base | 336.775 | 692.733 | 1536.689 | 2.056964 | +105.696% | 2.218298 | 334.054→687.495 |
| N256 k1 | 330.873 | 702.513 | 1563.474 | 2.123214 | +112.321% | 2.225543 | 327.389→698.757 |
| N256 k16 | 365.611 | 720.170 | 1675.970 | 1.969771 | +96.977% | 2.327187 | 358.125→719.131 |
| N256 k64 | 412.017 | 733.936 | 1637.789 | 1.781327 | +78.133% | 2.231513 | 405.198→730.859 |

8格B/A全部大于1.10，Rust/B全部大于.90，联合门仍全部失败。附加几何均值B/A=1.827635765、Rust/B=2.240553737与原summary一致，前者相当于约82.76%退化，不能用后者覆盖失败case。CPU均值同样增加约61.4%–113.4%，因此不能把全部wall差异解释为纯等待；没有据此进一步归因于具体某个优化pass、函数调用或调度因素。

## RSS与代码差异

216条wait4 RSS原始值全部为314488KiB，包括所有case/arm/预热/正式/额外功能。这些数值完整保留，但没有区分度，不能据此宣称三arm实际live内存相同、候选不增加内存或存在空间收益。

八case的A/B **ELF均不相同**；仅去唯一diagnostic fingerprint行后的sidecar均相同，分类与实际文件哈希一致。sidecar是partition前的整模块诊断IR，资源分区、导入和每unit优化发生在之后；它相同不代表实际优化单元输入或机器码相同。本轮是不同生成代码伴随显著运行退化，不能套用a60严格同ELF时的噪声解释。

源码审核通过和216次功能输出通过说明未发现这些控制下的语义错误，不代表少导入可无代价维持原跨函数优化质量。现有证据足以停止当前推广候选，不按观察值微调cost32、不删case、不追加轮数追求正结果。

## 后续执行边界

1679的runtime摘要是Failed，已审compile/concurrent的真实前置门应直接拒绝该receipt；候选编译收益矩阵与候选并发保持未执行，不能用潜在编译节省抵消运行退化。

root另行固定main==main的A/A加Rust并发诊断，属于无候选变化的基线能力测量，用于回答c1/2/4需求；它不是1679后续screen，不是绕过其runtime gate，也不支持1679采用。两条证据及身份必须各自保留。

完整六值、CPU/RSS、计数与审核hash见runtime-result-independent-audit.json；只读复算实现见recompute_runtime_1679.py。
