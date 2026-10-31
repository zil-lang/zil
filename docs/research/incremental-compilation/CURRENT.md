# 编译效率专项：当前交付

本轮从开工 main `21058d0e331de764e3a1b9123c3e1edf5e55a1aa` 连续推进到生产实现 `9fe5f55e64c7d537b9d585d3d40e42c9e248308e`。四阶段实现和一次实际 O2 误编译修复均已落地。专项分支 `research/incremental-compilation`，PR [#13](https://github.com/zil-lang/zil/pull/13)。这是有完整源码、反例、独审和测量的候选交付；父任务仍 Doing，未合入 main，也未改变正式产品门。

## 最终直接对照

下面全部来自同一个最终 main→9fe 窗口，10个 ABBA/BAAB 交错区组。时间为区组内每臂均值的中位数；收益是点估计，不是置信区间。全部44个单元见[完整表](LATEST-MEASUREMENTS.md)，不能把最大收益推广到所有编译。

| 场景 | main ms | 候选 ms | 结果 |
|---|---:|---:|---|
| 512模块，容量32，Session 未改动 check | 106.917 | 13.443 | 7.95倍，耗时降低87.4% |
| 512模块，容量32，Session 连续私有修改 check | 107.113 | 94.652 | 耗时降低11.6% |
| 128模块，容量32，Session 未改动 check | 20.484 | 3.536 | 5.79倍 |
| 128模块，容量32，Session 未改动 build | 55.531 | 35.479 | 耗时降低36.1% |
| 128模块，容量32，Session 私有修改 build | 57.856 | 53.192 | 耗时降低8.1% |
| 128模块，CLI 冷 build | 89.370 | 85.148 | 耗时降低4.7% |
| 32模块，CLI 首次私有修改 build | 41.604 | 43.519 | 耗时增加4.6% |

最终主线窗口5/44个墙钟单元不利：32模块 CLI build 的 private1/private2/restore/unchanged 分别约 +4.60/+4.17/+4.18/+0.58%，32模块冷 CLI check 约 +0.01%。真实混合源码15个结果等价单元中6个不利（约 +1.1% 至 +5.3%）；旧 byte_io O2 的错误结果另列，速度比无效。冷 CLI 和小语料没有普遍的大幅突破。

512模块私有修改的请求后RSS为65,284→64,734 KiB，未改动为65,364→63,272 KiB；128模块容量8192私有修改为50,500→46,742 KiB。它们是请求后观测值，不是请求内峰值。CLI 的峰值另按 wait4 记录。原始记录和不利窗口均保留，未用调大容量或删样本制造收益。

最终512模块未改动 check 的进程CPU为106.912→13.447 ms；私有修改为107.107→94.651 ms，与墙钟方向相符。CPU包含本进程编译线程，不含子链接器/原生控制进程，因此 build CPU 不能解释为完整构建树CPU。候选512模块未改动请求的 Verify 阶段间隔在20次请求中的原始中位数约9.953 ms（按10区组均值的中位数为10.045 ms），仍占主要成本；该事件间隔不是互斥CPU剖面，不能当作精确函数归因。

## 已解决的逻辑问题

1. **对象失效放大。** 原先把未使用的外部声明复制到所有CGU；修改一个无关声明也会使全部对象失效。现按结构化机器IR收集实际 foreign/global 依赖，保留未使用声明的合法性检查、全部 named types 和冲突回退。详见[首片](SLICE1.md)。
2. **会话容量断崖和过时数据并存。** 原始容量32的会话在28/29模块边界出现全清行为。改为有界原始结果淘汰、保留可达查询元数据、按逻辑费用收据检查当前 Parse 预算，并在新项目版本前释放过时大快照。原始条目数有界，不等于内存字节严格有界；其他项目可被保守淘汰。详见[组合实现](SLICE2.md)。
3. **重复拷贝、编码和遍历。** 移植并审查旧CP33/34/35的可用部分；流式保持原v27指纹字节，Arc共享未核Typed/Candidate，借用LIR视图，保留当前Verified绑定；LLVM named-type DAG成功高度记忆化，展开声明索引只建一次。旧DFS可沿共享类型重复指数级遍历。缓存里没有可跨请求授权的Verified结果。
4. **有完整前端结果却重做被淘汰的语法。** 对显式内存ProjectSources，完整当前输入键和有效依赖闭包命中时复用原始Typed结果，按原顺序重放可取消阶段边界；Analyze/独立Verify继续走当前请求。键覆盖全部源码/路径/顺序/ID/映射、11项预算、构建规则和目标；元数据损坏或前置不满足回原路径。文件系统发现、早停、dump和磁盘源码缓存路径保守保留。详见[完整前端复用](FRONTEND-REUSE.md)。
5. **同一批次共同父目录被反复解析。** 每次ArtifactPlan构建或validate内只记住最后一个成功parent，原destination函数作为miss oracle保持不变；每文件仍读取当前metadata/物理身份，每次新检查使用新缓存。没有跨请求缓存文件安全结论。详见[路径解析](PATH-RESOLUTION.md)。

这些改动修正了实际工作量失控，但还没有把私有修改变成函数级语义增量。发生源码编辑时，Expand/CT/Types/Analyze仍有整项目工作；独立Verify仍有线性成本。完整链接动作缓存、稳定语义分面和可独立核验的分段证书未实现。

## 性能调查中修复的正确性缺陷

真实 byte_io O2 程序在旧 LLVM API 后端输出正确但错误退出2，O0与clang O2均正确退出0。原因是负的窄整数传给LLVMConstInt时高位未规范化，LLVM23优化错误合并相邻常量。现按真实位宽先截断，再构造常量；未关闭优化、跳过核验或增加IR序列化绕路。

当前候选在所有混合源码参考控制中结果正确；旧byte_io O2的10个错误样本保留，速度比为null。另有窄整数各宽度、相邻常量、真实源码分支/循环、NUL/高字节输入与跨二进制对象缓存隔离控制，详见[阶段2报告](RESULTS.md)及`changes/research-narrow-integer-constants.md`。

## 窗口与证据边界

- 阶段3两窗口各5 blocks、1,560请求，main→5e与5cf→5e；5e仅增加测试，生产代码与a4一致。隔离窗口512模块私有修改109.426→124.151 ms（+13.5%）保留，没有拿另一窗口覆盖。墙钟不利单元分别6/44、19/44。
- 阶段4两窗口各10 blocks、3,120请求：main→9fe和a4→9fe。隔离窗口512模块暖check 23.154→13.896 ms、私有修改105.896→97.618 ms；4/44单元不利。每窗口含2,720 ZIL请求及400直接rustc参考请求，1,240次native控制；其中600份CLI输出留档，640次Session为probe内断言，不伪称独立输出文件。
- 新四窗口合计9,360请求、8,160次当前ZIL Verify、3,720次native控制。另8个真实源码的O0/O2共320测量构建/native和16个clang参考构建/native。真实资源/闭包/泛型/CT等消费者的前端复用集成测试另有48个构建/原生断言；不并入基准请求数。
- Session使用显式内存源码和虚拟文件路径（不落盘），生成器每叶8函数、根聚合叶调用；源码构造在计时外。当前ArtifactPlan物理检查仍在计时内，快速路径不代表文件系统CLI。CLI含进程启动、实际读文件；build含链接/发布，native语义检查在计时外。每个编译器臂使用相同请求序列和控制。
- 两个CPU核固定亲和性，CLI O0/8CGU/jobs2、对象缓存；Session容量32及128模块容量8192对照。rustc为1.99.1，直接O0/overflow checks/8CGU/持久incremental，metadata check不是Cargo check。ZIL固定LLVM23.1.4，rustc内建LLVM23.1.1；不作整个产品的Rust速度等价结论。
- 未清OS页缓存，未控温，也未给正式置信区间；每个区组保留，未事后剔除p95。阶段2最初的内存回退、后续修复、所有旧窗口继续保留，不能跨窗口相乘。

原始档案在`evidence/research/incremental-compilation/stage3/`和`stage4/`，含源文件/IR/报告/计数/原生结果/失败/时间行；不打包ELF和对象缓存。每档内部FILES.json和外部MANIFEST.json逐项绑定SHA。审核库存还保留当时机器上的ELF身份；重跑完整二进制身份检查需要复构或原工作区，归档本身足以重算原始统计。

阶段2的 [RESULTS.md](RESULTS.md) 与 [MEASUREMENTS.md](MEASUREMENTS.md) 冻结为历史结果，不重新绑定到9fe。最终新窗口的全部176+16个墙钟单元和48个Session CPU单元见 [LATEST-MEASUREMENTS.md](LATEST-MEASUREMENTS.md)。

## 验证状态

最终495项限定测试、受影响包all-targets Clippy、格式检查通过。独立功能审核分别绑定a4/5e/9fe：前端原始结果故障注入仍触发当前Verify拒绝；预算N/N−1、取消恢复、当前源覆盖保护和真实源码控制通过；路径审核另有696组destination结果及684组身份/诊断差分。三份最终Session probe由同一9fe源复构，ELF SHA与测量使用的三份逐一相同。

独立统计审查复算四新窗口全部1,599个已发布统计字段、全部请求/Verify/native计数、CPU原始转录和错误基线排除。原始数据复算最大浮点差1.42e−14。精确交付提交的审核收据另随交付保存，不能用测试条数替代产品验收。

GitHub自动工作流前一提交240bb37d8已全部通过（[run 41167705784](https://github.com/zil-lang/zil/actions/runs/41167705784)）。最终交付提交的自动检查状态以PR精确HEAD为准，推送后等待完成；该旧run不能证明新增阶段3/4已在GitHub验证。施工方只执行限定本地控制，未自行运行正式全量或修改产品门。

## 研究取舍与四目标

参考Rust的[增量查询](https://rustc-dev-guide.rust-lang.org/queries/incremental-compilation.html)、Zig的[增量编译进展](https://ziglang.org/download/0.14.0/release-notes.html)、Salsa、Adapton及Build Systems à la Carte；具体已读来源和证据等级保留于[audit/external-notes.md](audit/external-notes.md)。CP33预算收据、CP34流式指纹、CP35共享核验输入来自既有研究分支，按实际差异移植，未包装为本轮原创算法。旧CP32虽有高函数命中率，完整默认工作负载无净收益且RSS上升，不能以命中率替代端到端结果。

旧图切割和zit的可用思想是稳定owner、真实依赖边、相等结果截止、SCC/CGU分区及失败时事务回滚。要获得真正编辑规模的语义增量，必须先提供签名/布局/效果/CT/正文等稳定结果面和负查找依赖，再测分区收益是否覆盖摘要、哈希和调度成本。当前没有把旧内核整包搬入新语言，也没有以图算法绕过独立安全核。详细边界见[路线](PLAN.md)。

| 目标 | 本轮实证 | 未测/未完成 |
|---|---|---|
| 安全 | 当前Verify、预算、错误顺序、取消、源保护和O2正确性反例保留 | 全语言与Rust安全对等未建立 |
| 运行性能/内存 | 真正native行为控制通过，观察编译RSS | 系统运行吞吐、尾延迟、内存产品门未测 |
| 语法/维护 | 不改语言接受性、公共Wire、默认预算或Cargo.lock | 语法/维护成本对等未证明 |
| 编译成本及突破 | 明确暖Session场景约8倍；私有修改约11.6%，暖build约36.1% | 冷CLI普遍突破、函数级语义增量、四目标合取未完成 |

## 复现

使用云环境已保存的安装/启动配置，或自行准备Rust1.99.1和LLVM23.1.4。每个源版本使用独立target构建`zil-cli --features llvm-api --release --locked`；main210/a4/9fe的CLI身份分别见manifest。用同一9fe的`build_probe.py`分别链接三个版本rlib；复构命令和原/新ELF SHA见`stage4/validation/stage4-probe-source-binding.json`。

```sh
. /workspace/.zil-env/env.sh
python3 tools/research/incremental-compilation/combined_bench.py \
  --baseline /workspace/.zil-env/research/target-slice1-baseline/release/zil \
  --candidate /workspace/.zil-env/research/target-stage4/release/zil \
  --baseline-probe /workspace/.zil-env/research/stage4-main-baseline-probe \
  --candidate-probe /workspace/.zil-env/research/stage4-candidate-probe \
  --baseline-source 21058d0e331de764e3a1b9123c3e1edf5e55a1aa \
  --rustc "$(command -v rustc)" --blocks 10 --out /tmp/zil-main-final-new
python3 tools/research/incremental-compilation/mixed_bench.py \
  --baseline /workspace/.zil-env/research/target-slice1-baseline/release/zil \
  --candidate /workspace/.zil-env/research/target-stage4/release/zil \
  --out /tmp/zil-mixed-new
```

隔离窗口把baseline换成target-stage3、baseline-probe换成stage4-stage3-baseline-probe、baseline-source换成a4bec09fbaff29a353ad21571e183166de0968dc，输出使用新目录。脚本拒绝覆盖已有目录。源码交付后的文档提交不会改变生产代码；重新跑时manifest仍应记录实际HEAD，并单独说明生产树对应关系。
