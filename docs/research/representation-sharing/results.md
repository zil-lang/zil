# 068 完整文件入口配对测量

## 结论

真实重复工作已经删除，但这次有界测量没有建立普遍、稳定的整编译提速。所有18个组合都出现过候选较慢的配对样本，不能把单个中位数当成保证。真实 helper UI 的冷 Verify 配对比值为1.006，未变连续请求为1.032；合成17模块的冷 Verify 配对比值为0.928。后者是规模控制，不能替代应用证据。

以下全部数据来自同一冻结 schema10 身份，独立 Verify 的实际执行次数没有减少。后续 schema11 主线集成必须独立验收，不能把这里的时间改标为新版性能。

## 受测身份与范围

- baseline：`400a082444275e1d0e2ad78c4910b9b6fc526be5`。
- production candidate：`0f59d0ad176aac7b2ff0b9b7a4b1f951dacb7879`。
- 相同 runner 源码链接两个 production crate 集合，release、codegen-units=1、incremental=0、jobs=1。Rust1.99.1 / clang23.1.5。
- baseline binary SHA-256：`8114c36ed5ee8365e31ca67c96055e511ea8f3d13277230bf52b20e09adc346c`。
- candidate binary SHA-256：`4a45b868d84704b8ccc1f8145596efb77fd344c584918c3321c530fcad9db7a7`。
- 同一绝对源码路径、同一字节，避免路径参与接口/源码指纹时造成假不一致。源码与commit逐文件比对已保存。
- 3个输入 × Verify / Build / Build+emit × 2版本 × 1预热+7保留轮；每个进程先冷session，再未变连续请求。版本先后顺序按轮、case和mode轮换。
- 总共288次编译请求，其中252次进入保留统计；288次实际Verify；144对逐请求语义/产物对照、576个产物hash一致检查；192次真实构建后的native运行检查。
- root协调本任务组暂停其它Cargo/native/图计算；正式窗口23.69秒。没有声称整个共享宿主机无其它活动，也没有清理OS页缓存。

| 输入 | 唯一模块 | 源码字节 | 意义 |
|---|---:|---:|---|
| scalar | 3 | 135 | 很小的负向控制，固定成本容易淹没局部变化 |
| helper-ui | 5 | 3728 | 真实公共helper、拥有环境、资源清理、多模块文本UI；运行输入 `++q` |
| many-symbols | 17 | 77333 | 16依赖模块各8个私有body，以及确定的lexer padding；合成规模控制 |

## 完整API墙钟结果

`compile_path` 的区间从调用开始到报告返回，包含canonicalize/read、import发现及解析、source/产物预检、query cache、指纹、必要核验、请求的dump；Build还包含LLVM写入、外部clang和链接。每行是7个保留样本的中位数。配对比值先对同一轮 candidate/base 作除法，再取中位数，因此不必等于两个独立中位数相除。小于1表示该统计量更快。范围完整保留，没有删离群点。

| 输入 | 模式 | 请求 | baseline ms | candidate ms | 配对中位比 | 配对比范围 |
|---|---|---|---:|---:|---:|---|
| scalar | verify | cold | 0.854 | 0.849 | 0.962 | 0.832–1.128 |
| scalar | verify | unchanged | 0.180 | 0.174 | 0.953 | 0.635–1.422 |
| scalar | build | cold | 48.921 | 49.049 | 0.984 | 0.827–1.057 |
| scalar | build | unchanged | 48.454 | 47.949 | 0.953 | 0.885–1.087 |
| scalar | build-emit | cold | 61.928 | 63.219 | 0.993 | 0.791–1.059 |
| scalar | build-emit | unchanged | 60.976 | 61.243 | 1.001 | 0.884–1.059 |
| helper-ui | verify | cold | 9.622 | 9.482 | 1.006 | 0.861–1.375 |
| helper-ui | verify | unchanged | 1.408 | 1.427 | 1.032 | 0.782–1.430 |
| helper-ui | build | cold | 66.604 | 66.845 | 0.988 | 0.938–1.034 |
| helper-ui | build | unchanged | 58.598 | 59.260 | 1.065 | 0.938–1.241 |
| helper-ui | build-emit | cold | 139.720 | 133.260 | 0.982 | 0.856–1.510 |
| helper-ui | build-emit | unchanged | 132.061 | 122.645 | 0.935 | 0.889–1.104 |
| many-symbols | verify | cold | 13.308 | 12.961 | 0.928 | 0.703–1.706 |
| many-symbols | verify | unchanged | 2.437 | 2.501 | 0.992 | 0.688–1.045 |
| many-symbols | build | cold | 71.082 | 66.772 | 0.961 | 0.887–1.072 |
| many-symbols | build | unchanged | 59.160 | 56.166 | 0.983 | 0.714–1.100 |
| many-symbols | build-emit | cold | 189.644 | 188.646 | 0.963 | 0.930–1.216 |
| many-symbols | build-emit | unchanged | 180.097 | 178.993 | 0.984 | 0.842–1.095 |

这里没有给出显著性结论，也没有将7个小样本的波动解释成因果证明。三项生产修改一起比较，不能用总差值拆出某一次clone、parse或hash各自贡献的时间。准确可证明的路径操作次数另见 `vertical-sharing.md` 与 `results.json`。

## 内部报告与完整进程

内部 `CompileReport.seconds` 在文件入口重新覆盖计时起点，也包含前置加载。全部288次请求中，外层API时间减内部报告时间的中位数为 **2.854微秒**，最大为 **1522.434微秒**；它不是遗漏整个加载阶段的“内部核心时间”。

以下进程墙钟包含同一个进程内的cold+unchanged请求，以及启动、runner报告序列化/写入和session销毁；native行为检查由控制器在计时之后执行，不计入这些区间。不能把进程残差都算成编译器某个阶段。

| 输入 | 模式 | baseline进程中位 ms | candidate进程中位 ms |
|---|---|---:|---:|
| scalar | verify | 2.532 | 2.483 |
| scalar | build | 98.042 | 97.458 |
| scalar | build-emit | 123.442 | 127.917 |
| helper-ui | verify | 13.310 | 12.713 |
| helper-ui | build | 127.356 | 127.363 |
| helper-ui | build-emit | 274.046 | 260.301 |
| many-symbols | verify | 17.687 | 17.340 |
| many-symbols | build | 132.328 | 128.265 |
| many-symbols | build-emit | 380.118 | 363.267 |

## 等价与失效控制

- 新增6项限定集成测试，加既有19项driver单测和27项project测试，共52个不同测试通过；不累计前后重复运行。
- 请求路径与显式source graph的十种stage字节和已核接口一致；cold、unchanged状态都实际Verify一次。
- 累计source字节、token、node、import深度的不足限额拒绝，准确边界接受；Parse-only不发现missing import。
- 预先取消、输出与依赖输入碰撞，不发布任何raw查询；结束active query数为0。
- 已存在只读source缓存读取不改mtime；fresh session仍执行raw查询和独立Verify，磁盘内容不能替代分析。原有磁盘损坏及篡改typed缓存负向测试也通过。
- 私有正文改变、target、build inputs、analysis budget改变均触发预期失效；原有取消中途/after-hit/after-link测试保持。
- 所有正式对照匹配query events、完整接口、source身份和请求的文本产物；Build另以明确预期检查退出码、stdout/stderr。

## 原始失败与可复现性

首次fmt检查发现两处排版差异，保留输出后修正。第一次预检实际使用正确基线二进制且功能比对通过，但手填的baseline commit标签有误；那份身份记录和时间不作为正式证据，未经篡改保留。正式控制器先验证完整commit对象，再进入记录。所有预热、回退样本、源码hash、命令、native输出、dump和两个runner二进制保留在本片portable raw archive；摘要及hash见 `evidence/impl/068`。

重放时在相同工具链下把同一 `tools/research/068` runner链接冻结baseline及candidate，给 `measure.py` 两个二进制与真实完整commit；它在同一输入路径上依次执行。runner输出目录必须不存在。正式计时前另行安排独占计算窗口，不能把重编译二进制与采样并行。

后续优先研究结构化IR指纹、稳定symbol/type数据的不可变共享、准确的函数级依赖和必要核验后的表示复用。这些方向需要独立实现与测量；当前还没有把它们算入收益。
