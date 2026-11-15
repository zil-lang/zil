# 当前接续：完整调用链归并、导入与构造前裁剪

实际编译源码 **027695318436fcb1f8994d0dc9802a0b2349c362**，binary `5b3a309a7860dca3a1d3f15e7c930ec7d3f3a3fa59286e6d91c67e0067c0743f`；工具05d4b276f。唯一施工分支research/full-push-native，actual main9170，研究默认关闭，合入Hold。源码与下面所有新性能数字只绑027，不归给98fa/021997/e677/065。

已按整条链实施：完整当前标量体→受限整数直接调用DAG精确等价类→完整导入/当前内部化→检查原完整unit后裁剪不可达local函数→真实16unit的preflight/Builder/full O2/LLVM Verify→发布/编辑/恢复。函数hash仅选择，完整Function等价授权；地址/入口/未知/循环边界保留。当前独立语言Verify、原始未裁体检查、预算和O2未减，所有研究入口默认关闭。

原始102文件项目：1000/2500/5105声明；两平衡block、完整18请求含8私改，每请求恰一次，无行业频率杜撰。actual main/候选/Rust，CGU16/O2/jobs1，check与native分开。下表为完整流程等待中位数和用户产物物理边界峰中位数。

| 规模 | main等待s | ZIL等待s | Rust等待s | ZIL省等待 | ZIL/Rust磁盘峰MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1000 | 8.246 | 6.412 | 8.424 | 23.9% | 9.061 / 19.176 |
| 2500 | 11.748 | 7.106 | 9.307 | 23.6% | 12.846 / 36.670 |
| 5105 | Expand限额 | 26.296 | 31.668 | 17.0% | 46.820 / 72.855 |

5105冷3.835/Rust6.719s，broad2.758/5.024s，restore1.370/5.450s；8私改10.754/9.859s、interface2.688/1.852s、重启2.232/0.0217s仍输。1000/2500接口和重启同样有缺口。只报告同窗完整流程净收益，不以平均掩盖单场景失败。

8私改磁盘增长1000为1.186/Rust5.125MiB，2500为1.041/11.527MiB，5105为1.484/28.480MiB。口径为用户cache/最终产物/诊断IR，lstat/st_blocks全局inode去重、请求边界峰；非构建瞬时峰，不含SDK、宿主target或研究证据。fresh恢复不保持hardlink拓扑，不重算磁盘收益。

324原始请求/108功能格完成；51个按原Source身份绑定的10000循环持续运行用例全部过Rust门，R/B几何均值4.131。main1000/2500运行回归门过，main5105 Expand失败使该回归未知，所以summary.accepted=false、candidate_rust_accepted=true。每个原始成功exe实际EOF+256字节oracle；功能并发8仅在编译计时之外，正式runtime串行1warmup+6平衡次。1.10/.90/.95门不变，六样本p95/RSS不支持稳定尾延迟或内存优势声明。

扩大到10105/25105声明：102文件、100×100/250×1原生成输入，无新增unused或裁源。check/native分开、两block七请求。实际main和027在全部快照默认lex token预算拒绝，Rust检查/原生观察成功；56个native ZIL限额报告、84请求完整、0基础设施失败。容量未过，不给无效构建时间/磁盘比；大规模native期间宿主归档清理重叠，整个容量批次时盘性能均排除。首次check工具误拒绝零query的合法lex限额，首败保持incomplete，56原响应经独立重放形状/身份验证，未冒认新检查执行。

集中门：5af2实际backend291/driver219/Source9/publication1过；027只等价let查找转?，独立复核后按差异复用，未冒认重跑。当前027 strict/fmt/release实过；首个strict lint失败保留。工具05d实际19测试过；失败window04旧layout误选及原工件重放保留。见[算法独审](call-classes-reviews-20261115/algorithm-review.md)、[交叉集成复核](call-classes-reviews-20261115/integration-review.md)、[完整加权数字](call-classes-reviews-20261115/weighted-coverage.json)、[容量缺口](call-classes-reviews-20261115/capacity.json)。

负结果不抹去：closed-only e677的51运行门全失败；complete-import e677运行过但全流程9.018/11.810/49.025s输Rust8.224/8.917/33.004s；native140捕获30.972→31.272s无净益已回退；065的31.649→16.706s仅e677/065诊断，不是027/Rust正式数字。前述完整证据分别固定948/5af2恢复依赖。

[本批恢复入口](evidence/call-classes-20261115/RESTORE.json)保存当前全原始正负、027/065二进制、集中门/失败/诊断、控制器与独审。SDK18/native-link固定V32依赖，不重建LLVM。历史宿主证据/恢复副本只在committed manifest、全parts、fresh收据与每原文件完整hash核对后退役，不计产品磁盘收益；恢复路径以各archive为准。98fa正式结果仍固定59fd，绝不归当前源码。

尚未全面超Rust：1万/2.5万默认容量失败是优先级最高的硬缺口；当前三族是有限合成Source，不代表工业workspace、泛型、宏、async或分钟级ZIL成功。下一完整链处理容量合同与fresh机器构造/partition/cache恢复共同成本，先解决不能完成和绝对秒数，保留原始义务与默认预算；goal与研究归档不合main。

恢复验收：正式窗35份867,164,161bytes、236,551files/3,985,638,723rawbytes；源码/门/失败/容量66份1,655,437,572bytes、33,414files/5,737,021,115rawbytes。两包均从空目录逐文件完整验证通过；只退役刚验证的fresh副本，当前原始证据与全部parts仍留存。首次合包超过既有64MiB inventory安全上限已保留首败，按两份证据容器恢复，未修改packer限额。

2026-11-16按Rust真实痛点复核：[行业痛点/时间空间与源码对照](rust-pain-reviews-20261116/review.md)、[Astra实际Rust实现审核](rust-pain-reviews-20261116/rust-implementation-review.md)。Rust58cf/Cargo5f94逐项固定；Rust已有session共享/回收和O2合并，改进应对照其实际查询传播、早期CGU复用与按需存盘。当前保留缓存更小但重启慢，未证明等空间/清历史后仍快。除token外，全项目默认10000函数使10105/25105原输入在合同不变时必然不合；不能许诺只优化lexer即恢复容量。后续每个落后点和主动优化均先附实际Rust对应文件/函数与代价差异，完整链完成后统一验收，数据仍绑027，未新增编译/测量。
