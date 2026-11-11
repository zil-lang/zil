# Full canonical O2 producer：完整执行后的运行质量 NoGo

## 决策

**本轮36编译/108native完整执行，输出全部正确，但两个case的运行质量均越过10%相对退化门，总体NoGo：不采用，不扩大矩阵。** summary和execution-manifest是Complete，不是Incomplete；driver exit1来自固定运行门的最终SystemExit，而非编译/收据失败。N256编译wall/CPU真实改善保留，不能挽救其运行质量失败。

本审只读全部原raw、冻结脚本、源/工具身份和既有构建日志；没有编译、LLVM/native执行、追加采样或修改raw/冻结源码。报告与完整JSON在冻结ROOT外。

## 协议、工具与数据完整性

protocol `out/next-optimization/full-producer-protocol-v1/protocol.json` SHA `360635bffc5dc4230bd2c9ed2ca18e66c8edd29139e17a910afc559c03c0147f`；raw `out/next-optimization/full-producer-screen-v1/`。协议副本原字节相同，全部scripts SHA/currentbytes匹配；cases完整对象、640份actual sourcehash、251seed oracle/vectors与原a30 reference一致。CPU0,1/jobs2、100000循环、ABR/BRA/RAB/ARB/RBA/BAR六排列、36compile/108native/36formal和原B/A≤1.10/Rust/B≥.90不变。

实际913个raw文件首次读取、核心复算与详细图/分支复核后完整重新枚举SHA全等；JSON保存每条path/SHA。146份记录的attempt＝36compiler+108native+2version，全exit0/reaped、无timeout/exception/residual，逐份与raw-metrics相等，无failure JSON。它们不是全部OS子进程清单；内部普通Clang工作已经计入外层compiler wait4。不存在省掉失败、遗漏round或过滤invalid后算Pass。

A main990/tree1c16744d，B source2458defcaf8be51c87b80c0dadf391bb585735e8/treee917926bc8976da90e58897911c72978b9a24f88/binary30140135c39706e8251bd5da3eb9acb4e8481a9c12acbc4aed6403f57a768097，两个flat source/tree/binary/release绑定逐SHA核对。B完整bindingcab4b8644dc992c300f18872737663f5f8a6f4d784e32a7f6a75d3cba743c34a的四attempt/command/stdout/stderr也核对，API各group合计64passed、proof合计6passed、均无failed/ignored；no-native/release真实exit0，原warnings保留。没有重新跑这些测试，旧ce90681f不冒认为本候选。

所有四工具actualbytes、实际LLVM DSO72146000bytes/path/resolve/SHA2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250核等。ZIL/Clang/nativeLLVM为23.1.4，Rust1.99.1的bundledLLVM为23.1.1，rustc-vV原文核实。protocol的LD_LIBRARY_PATH为本SDK lib目录；冻结driver实际逻辑在screen前后校验此环境/DSO，逐arm清旧ThinLTO/private环境、仅B设置新绝对receipt路径；actual编译argv逐行是正确binary、llvm-api/O2/incremental/after8units2jobs，无cache。raw没有完整环境或/proc/maps抓取，本审不把协议与源码行为冒称独立loaded-map观察。

## 数学、产物与运行样本

独立遍历全部251seed，直接重算原四步recurrence及八函数/leaf之和，使用100000=398×251+102聚合mod1000003，得到00/ff/41三输入小端结果；未调用作者fixture或oracle helper。72功能+36formal实际stdin/stdout/stderr全部符合数学模型。每一compile/native行的case/round/position/arm顺序、program SHA与实际文件均相等；每arm每case六轮ELF各唯一，A/B各自materialIR去唯一verified-fingerprint后跨轮相同。跨arm允许不同码，没有套用旧同ELF门。

## 当前模型、实际图与分支

24次current语言Verify；A96个当前unit预检/LLVM before-after；B12原模型、12分区输入模型和96当前unit预检。每B首object与独占外部current-preflight完全相同，后exact8objects；原typedComplete current flags、唯一canonical owner、原Required=Allowed、每函数/每unit/总copy费用界保持。

八unit actual definitions/declarations/direct_calls/intrinsics组成全图，本审独立用定点可达重新计算Required，不调用作者DFS/validator；所有call/declaration仅限owner、精确getchar/putchar和实际LLVMGetIntrinsicID名单，不按llvm.*泛放行。每轮N64 source/copycost35777/31360、根Allowed504、Required56；N256144449/125440、2016、Required224。实际Required⊆Allowed、imports精确排序覆盖、固定私有名和正direct-use数、donor_modules按owner数均核等。十二请求共1680imports/directuses、84donor modules。

| 实际阶段/路径 | N64六轮 | N256六轮 | 总计 |
| --- | ---: | ---: | ---: |
| 当前full default<O2> canonical producer | 48 | 48 | 96 |
| actual Required非空，ran=true | 6 | 6 | 12 |
| actual Required为空，ran=false直接emit | 42 | 42 | 84 |
| actual Required为空但原Allowed非空 | 0 | 0 | 0 |

96份unit/2全部pipeline=default<O2>、ordinary-no-summary、正fullO2 optimize计时、当前structured preflight与before/after verify（192个标记）。96个object/2全部actual_required_path_verified=true，无旧verify_parse_trim_link_private_postlink字段；official importer=false/nativecache=false/summary-index=false。对象pipeline精确为canonical-full<O2>|budgeted-direct-imports=private|empty-required=emit|nonempty-required=thinlto<O2>|merge-functions=after|aliases=false，标准LLVM/CPU/PIC/布局/emit字段符合固定实现。

空Required84unit：postlink_optimization_ran=false、object.optimize_ns=research.postlink_optimize_ns=trim_link_ns=0、donor0/imports空，parse/emit仍真实正计时且当前path_verified/LLVM before-after为true。非空12unit：ran=true、两处optimize_ns正且精确相等、trim/link正计时和7donor；本审依据独立Required选分支，未用Allowed为空偷换条件。当前screen不覆盖“Required空但Allowed非空”实际unit，该控制只存在既有API/source测试，不能说本screen也覆盖。

24个ZIL report.commands均只有普通pinclang-O2目标链接和8objects，无flto/plugin/ThinLTO link参数。没有旧externalopt/dis/summary/HASH或pluginhook计数；不把96 emitted objects说成96次第二O2。逐Use及parse/check_sealed/原生LLVM模块校验依赖已冻结native源码非作者审和实际API收据；外部对sealedfacts的图/预算/coverage独立复算，不虚称另解码bitcode、全文指令等价或从目标集合还原Use multiplicity。

## 本轮完整编译统计

单位ms；每cell六完整正式compiler请求，所有wall/CPU/RSS数组、median/max/sample p95保存在JSON。n6不支持稳定尾分位估计，未删长尾。

| case | arm | wall mean | CPU mean | RSS mean KiB |
| --- | --- | ---: | ---: | ---: |
| n64/base | A | 1165.224937167 | 2110.902666667 | 291116.000 |
| n64/base | B | 1447.139819167 | 2127.768000000 | 291116.000 |
| n64/base | R | 902.797088667 | 1226.981333333 | 291116.000 |
| n256/base | A | 4486.522899500 | 7893.615166667 | 414647.333 |
| n256/base | B | 4061.067250000 | 6848.031500000 | 464320.000 |
| n256/base | R | 2828.954318833 | 3995.259166667 | 291116.000 |

| case | B/A wall | B/A CPU | Rust/B wall | 编译局部结果 |
| --- | ---: | ---: | ---: | --- |
| n64/base | 1.2419403096 | 1.0079896310 | 0.6238492485 | wall与CPU均不利 |
| n256/base | 0.9051702936 | 0.8675405825 | 0.6966036622 | wall与CPU均改善 |

N64完整compile wall+24.1940310%、CPU+0.7989631%；N256wall−9.4829706%、CPU−13.2459417%。这是本轮各自A/B完整请求的结果，不是相对manual旧session的成对提速。N256编译RSS B均值464320KiB高于A414647.333KiB，不用runtime launcher floor掩盖此项。

## 运行质量门：两格明确失败

| case | A wall mean ms | B wall mean ms | R wall mean ms | B/A≤1.10 | Rust/B≥.90 |
| --- | ---: | ---: | ---: | ---: | ---: |
| n64/base | 112.229907167 | 150.944054000 | 419.465772500 | 1.3449539237 失败 | 2.7789486328 通过 |
| n256/base | 321.829176500 | 470.603988167 | 1593.260589667 | 1.4622788191 失败 | 3.3855654217 通过 |

B相对A运行wall分别退化34.4953924%/46.2278819%，均明显超过10%门；Rust/B通过不能挽救。输出正确是功能门，不是运行性能门。全部formal native原始wall（ms）如下：

- n64/base A：121.434217, 112.435394, 109.865765, 107.005330, 115.575772, 107.062965
- n64/base B：162.947103, 150.555716, 160.862568, 135.576791, 147.998508, 147.723638
- n64/base R：396.738012, 409.720176, 415.282011, 385.492858, 399.565139, 509.996439
- n256/base A：320.220066, 315.642006, 316.017107, 335.842632, 318.813967, 324.439281
- n256/base B：461.598347, 462.933363, 453.936809, 468.899849, 466.993393, 509.262168
- n256/base R：1536.396994, 1591.796402, 1528.553468, 1512.709350, 1586.417980, 1803.689344

nativeformal RSS均291116KiB，有launcher/fork floor，不是整个进程树同时峰值或实际内存等同证据。

## 归档边界

执行完整、验证有效、功能正确、N256局部编译改善和两格运行质量失败同时成立；本版总体NoGo、不采用、不扩大矩阵。没有为消除退化调O2/阈值/预算、挑case或删长尾，没有把exit1改叫Incomplete。actual阶段字段只能证明路径执行，不能单靠零ns跳过数解释运行回退的具体机器码原因。本报告不归咎环境，也不提出用loop等启发式回退掩盖门。旧manual等结果独立保留，不被本版覆盖。

独立JSON SHA256：`e6280cf7b6a00144e6643639d70c93656f6830b868ff3ea2ec4ccd2f56b0a23f`。报告完成后raw保持冻结。
