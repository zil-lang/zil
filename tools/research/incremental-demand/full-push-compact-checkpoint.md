# 当前接续：机器生成与连续编辑磁盘

实际编译源码 **789f879f249c7f45a5c84ce062be30f9c0985740**，binary `e9bcf50ceb40a3f5e86b9e5abd4793a4302e9e92b279917b1f4da7b49d693274`。唯一施工分支research/full-push-native；main仍9170，研究默认关闭，合入Hold。

已实施完整current-module闭合整数体机器生成复用、同subtarget重组装，以及无损共享epoch metadata。完整preflight/Builder/O2/LLVM前后Verify和当前独立语言Verify保留；原module不改，未知/失败回普通路径。大体alpha只在有同结构候选且局部名字不同时付print-clone成本；hash仅选择，不授权。

[当前789测量](critical-path-reviews-20261114/critical-path-wave06-weighted-coverage.md)：2500声明17请求，102行完整ELF过；main/候选/Rust总等待 **10.622 / 7.231 / 7.906s**。冷 **2.301 / 2.354 / 1.426s**，broad **2.199 / 1.802 / 1.238s**，仍慢Rust；8私改贡献main/候选/Rust3.807/1.770/3.100s。按绝对秒数和真实实验频次，不称每项领先。

本窗用户产物物理边界峰 **26.906 / 12.504 / 42.508MiB**，8私改增长14.875/1.094/11.527MiB。[5a的32编辑检查](critical-path-reviews-20261114/critical-path-wave04-weighted-coverage.md)增长46.625/**2.752**/3.090MiB、峰53.738/8.145/12.160MiB；旧1de增长8.64MiB。5a测量仍绑5a，未重标当前789；其metadata生产代码未再变化。仅用户cache/产物、全局inode去重、请求边界峰，非构建瞬时峰，不含SDK/宿主target/证据。

冷请求每block实际2092次机器函数复用、norm0、无fallback；整段5516事件/block、后续norm10/block。alpha扩域本身未增加实际命中，[769负结果](critical-path-reviews-20261114/critical-path-wave05-weighted-coverage.md)保留wide8.360/Rust8.206与diamond8.508/Rust8.350s，不把跨窗口差额当净收益。

原5105声明/102文件/843875bytes全部原字节保持，当前默认预算check/O2冷暖均当前V1，候选/Rust各24native通过；main四次Expand限额，非性能比。当前容量诊断cold10.172s、其中object7.840s，是比毫秒暖命中更优先的绝对成本，仍需正式同类窗口。

本轮三源码共12窗9过/3千函数原生完整ELF失败、1071行；失败comparisons空。每窗只实际main/对应候选/Rust，同CGU16/O2/jobs1，检查分开。八个ordinary早标量SSA函数1584字节差异已定位；旧失败、当前norm38/reused0及未执行历史不抹去。main最小stock30c暖退化亦保持Hold。

本源码emission15、strict/fmt/release实过；API99组成旧97成功+当前15含13重叠，不冒认全99重跑。backend273/driver219/Source9/publication1实际d71收据按独立精确复核复用，当前大项目与5105另有真实Source链。完整独审见[源码/首败/成本复核](critical-path-reviews-20261114/post-o2-emission-exact-review.md)与[metadata独审](critical-path-reviews-20261114/shared-manifest-exact-review.md)。

[首批恢复](evidence/critical-path-first-20261114/RESTORE.json)：42份1,047,090,662bytes，40,560files/4,353,188,686rawbytes；[最终恢复](evidence/critical-path-final-20261114/RESTORE.json)：42份1,045,814,087bytes，47,618files/4,368,816,197rawbytes；两套均真实fresh全文件验证，SDK18/native-link固定V32依赖，不重建LLVM。98fa正式结果仍固定59fd，绝不归给021997或之后源码。

尚非工业大型workspace/generic/proc-macro/async或分钟级痛点覆盖，不能宣称全面超Rust。下一整链优先原5105的7.84s机器对象成本：验证现有显式late-closed-executable合同下零用户体DCE，完整源码/分区/独立V/O2不减；另冻结非同字节运行质量协议，旧ELF失败照旧保留。goal/研究归档不合main。
