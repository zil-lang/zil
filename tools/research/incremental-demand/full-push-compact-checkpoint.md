# 当前接续：实际依赖、结果截断和当前分区条目复用

当前编译源码 **f19fc847c5774a2472bd3af85019b5c937546010**，tree `c4aeeccf0ba4bfd92d2fb2587365444d75bed105`，binary `68867c4c0333cbceac34e6d688a9581702c7ef337b4ace34a88458df21e72e85`；冻结工具0f9d4d78e。唯一施工分支research/full-push-native；actual main9170保持。下列新数字只属于f19，不归027/021997/98fa。SDK18/native-link沿固定V32恢复，没有重建LLVM，研究入口仍默认关闭，main合入Hold。

本轮已施工：Expand按实际标量namespace保留未受远程签名影响的产物；CT按函数记录真实正/负/歧义hint读取。后端按FunctionId索引及自身/直接callee完整headers复用，结果相等截断仍绑定原Source调用槽位。完整当前planner继续选择CGU和导入闭包；实际旧条目只有完整输入、owner/role和私有导入名依赖均相同才能复用。当前内部化之后，完整未裁unit与配置相等才复用已检查裁剪结果。独立Verify、当前机器授权、完整O2/LLVM Verify、原始预算不减；没有新增磁盘Source→Machine授权。

f19实际backend299、driver219、真实Source9、native publication1，以及strict/fmt/release全过。Expand124/CT76在f629实际通过，f19仅改后端测试fixture和精确计数，按差异复用这200项，不冒认重跑。首败Expand121/3、backend293/6及未启动compiler的controller03错误全部保留；非作者和Astra复核见[native-dependency-reviews-20261116](native-dependency-reviews-20261116)。

正式同窗actual main/候选/Rust、O2/CGU16/jobs1：原102文件、两平衡block、18请求含8私改，每请求一次，无杜撰频率。324原始请求/108功能格完成，51个原Source绑定的持续运行用例全部过Rust门，R/B运行几何均值4.137；这不是编译提速比例。main1000/2500运行门过，main5105仍Expand限额，使main回归未知、summary.accepted=false而candidate_rust_accepted=true。

| 声明规模 | main流程s | f19流程s | Rust流程s | 省等待 | f19/Rust用户磁盘边界峰MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1000 | 8.288 | 6.620 | 8.877 | 25.4% | 9.078 / 19.176 |
| 2500 | 11.514 | 8.212 | 9.271 | 11.4% | 13.145 / 42.516 |
| 5105 | Expand限额 | 24.823 | 31.119 | 20.2% | 46.852 / 72.855 |

5105冷3.623/Rust6.065s、broad2.791/4.970s、restore1.310/5.406s领先；八私改10.962/9.894s、接口2.195/1.800s、保留缓存重启1.273/0.032s仍落后。2500 broad1.386/1.164s、接口1.009/.780s；1000接口.755/.677s同样落后。六份当前Rust重启stderr均Fresh且无rustc。新窗2500流程8.212s相对旧027窗7.106s更慢，不能隐藏；两窗不是配对因果实验，旧027结果仍固定原提交和原Rust控制。任何完整流程领先均不关闭单场景差距。

8私改磁盘增长f19/Rust：1000为1.223/5.125MiB，2500为1.199/11.527，5105为1.441/28.480。计量含用户semantic/object cache、最终产物和诊断IR，lstat/st_blocks全局inode去重、请求边界峰；不含SDK、宿主target、研究证据，不是瞬时峰。恢复不保持hardlink拓扑，不能由解包后文件重算产品收益。同版独立check与保留/全清缓存实验已完成，见后文；仍未测试保留当前版本的部分历史回收与等空间配额。

真实组合缺口已保留：positions→interface同时改变导入位置和远程签名，两个外层attempt gate合取失配，使100个有效placement未重绑定；CT还需要真实producer到当前Core owner的桥接，不能仅忽略span或借旧owner。5k/private002实际codegen980.56ms=machine274.09+partition186.25+objects483.98+link21.03，CT14.71、Expand46.95、Verify48.01ms；后续按重复私改的绝对累计成本优先，不先扩小访客框架。此族native产品捕获由既有阈值39.5<64关闭，不将这些收益归给正在命中的SDK18函数捕获，也不重推已否定140启发式。

原10105/25105容量仍受默认1M token和全项目10000函数硬合同拒绝，数值/语义不改则不能准入。有限标量单crate/O2/no-debug并不覆盖工业workspace、泛型、宏、async、debug/test或分钟级项目；未全面超过Rust。每个主动/落后优化继续对照实际Rust58cf/Cargo5f94，见上轮[真实实现复核](rust-pain-reviews-20261116/review.md)：Rust已有result-green、早期CGU判断、按需恢复、session锁/共享/GC、ThinLocal及O2 MergeFunctions，不能假定其没有。

独立check同版完成三组，每组17请求×两block×三臂，共306请求/102检查格；原102文件，simple函数，默认预算不变。check与native不混合计时：

| check场景 | main流程s | f19流程s | Rust流程s | f19/Rust磁盘峰MiB |
| --- | ---: | ---: | ---: | ---: |
| 2505函数·chain | 3.038 | 1.917 | 3.560 | 8.711 / 29.672 |
| 2505函数·diamond | 3.838 | 2.523 | 3.676 | 9.891 / 29.672 |
| 5005函数·wide | 6.866 | 4.736 | 8.376 | 16.701 / 58.348 |

两份独立缓存生命周期窗各72原始请求/24功能格/10运行用例，20个Rust运行门全部通过。本轮合计774原始请求/258功能格/71运行用例。六请求历史为cold、unchanged、真实restart、private001、error、recovery；clear发生在首个冷版本之后，不是八私改后的部分历史GC。保持原Source身份、O2/CGU16/jobs1及全部运行门。

| 5105缓存状态 | f19/Rust重启s | f19/Rust随后首改s | f19/Rust两请求合计s |
| --- | ---: | ---: | ---: |
| 保留 | 1.597 / .0239 | 2.773 / 1.508 | 4.370 / 1.532 |
| 全清指定cache | 3.798 / 6.386 | 1.196 / 1.275 | 4.994 / 7.661 |

保留仍比全清少.624s，不能因清空后单次修改较快就建议清空。clear2500重启1.858/Rust1.644s仍负；retain2500首改.747/.502s仍负。清理释放5105候选约10.54MiB/Rust29.86MiB；候选program/IR保留、Rust target含exe删除，范围不同，不能冒认对称全产物清理。cleanup/startup均单列于编译timer之外，candidate边界delete_ns同时包含两方删除，不能分摊为某方独立清理时间，也不杜撰端到端总等待。详见[时间空间原数字](native-dependency-reviews-20261116/cache-time-space.json)。

新定位：Whole-Typed磁盘命中在真实Parse/Expand/CT之前返回，epoch=None；下一编辑需重新建立上游细粒度状态，虽5104 backend值命中，representation/sourceprojection仍5105重检。磁盘仅TypedProgram+CandidatePlan，没有上游parser/producer owner、真实依赖与费用；不能以当前Typed Verify、hash或新分配Arc冒充上游授权。当前无错误结果证据。[实际路径](native-dependency-reviews-20261116/native-dependency-restart-next-edit-review-01.md)与[Astra独审](native-dependency-reviews-20261116/native-dependency-retained-restart-astra-review-01.md)对照Rust58cf try_load_from_disk/cache.complete：加载哪个query就发布哪个query，而非从下游值反推上游。仅正常初始化前端并保留对象cache是可测保守实验，收益未知，必须合计restart+edit；不把重建移出timer冒认优化。

下一完整链按上述绝对秒数处理可安全恢复的真实输入/依赖/费用与下游局部transform/class/ref facts，保留当前global代表/导入决策和完整原检查。当前一次私改machine+partition八次共4.221s只是成本桶，不承诺全可省。当前研究分支含工具/证据及大量生产差异，未完成向main的干净发布与正式产品门，整枝合入Hold；不能因有限流程领先关闭全面超Rust目标。

完整证据已保全：正式窗35份870,498,170bytes，236,872files/4,007,054,729raw；门/首败/check/cache及恢复49份1,228,271,760bytes，106,448files/4,777,702,955raw。两包均从空目录逐文件完整验证，验证scratch已退役；当前check/cache raw保留。正式raw重复本地副本在[固定afc97106d恢复](https://github.com/zil-lang/zil/blob/afc97106d3b9ce9dc8eaa5a529c3dde72900e199/tools/research/incremental-demand/evidence/native-dependency-formal-20261116/RESTORE.json)、所有parts/fresh收据及每个原文件hash核对后退役，ledger在[当前完整恢复](evidence/native-dependency-20261116/RESTORE.json)内；不计产品盘量收益。两个payload根不同，按RESTORE映射分别恢复，SDK18/native-link仍固定V32，不重建LLVM。

旧027正式窗固定[05f270](https://github.com/zil-lang/zil/blob/05f270ebe13169ad7a089ad98d6d327bab834bcd/tools/research/incremental-demand/evidence/call-classes-formal-20261115/RESTORE.json)，旧源码/门/容量/负结果固定[53c077](https://github.com/zil-lang/zil/blob/53c077e723fbd3aa43e96962260d4834d33ce96b/tools/research/incremental-demand/evidence/call-classes-20261115/RESTORE.json)。旧window05和两容量raw也已按相同全hash规则退役，不假设本地仍存在；完整原字节均可恢复。98fa正式数字仍只绑98fa/59fd，goal/archive不合main。
