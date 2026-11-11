# 原生进程内 prelink a299：独立原始结果复核

## 结论

**固定两case运行质量Pass，完整编译两格均NoGo；协议未失效。** 本轮N64/N256的完整编译wall B/A为1.2246814020/1.0922135826，CPU为1.0176928371/1.0540271094，不能以运行门通过宣布编译收益或采用。所有原始样本和native长尾保留，不从已有请求扣阶段时间；与旧wrapper/identity/linear是不同session，不计算跨session成对提速。

本审只读取已有raw/收据/冻结代码和日志，未编译、执行LLVM/native/plugin或新增计时，未修改冻结ROOT。独立结果与每条raw SHA在`research/manual-prelink-result-review.json`。

## 固定身份与实际次数

protocol `out/next-optimization/manual-prelink-protocol-v1/protocol.json` SHA `a2999b0eb482994de4516f8de6760ca6dd4a7b99fe722c8b0d75aaa9daa76810`；raw `out/next-optimization/manual-prelink-screen-v1/`。raw中的协议副本逐字节相等，脚本逐SHA匹配已发布harness850289f5a732e58cf03ee79f8a2c8b1b3ae3a489/tree2d20f1ed91d6347e28f00b31b7d20632250f1c54所冻结清单，producer独立为2321e74897975978925dc7d2a611f7d672f716e1/tree03fa3d83dc5ba80d7849f7148b97b7d12449700b，B binary ce90681f261c109c412bb9d513e3c0ff58680a2e593d798cdf21b3e27daa407a。

实际**913个raw文件**首次读取与全部数学/统计/图审核后重新枚举SHA全等。实际记录**146份process-attempt**＝36完整compiler请求＋108native＋2工具版本命令；全部exit0/reaped，无timeout/exception/residual，与各自raw-metrics相等，无failure记录，execution manifest Complete/completed=true。不是旧930attempt/6125文件；146是记录次数，不声称包含每个内部Clang子进程的独立attempt。final Clang实际工作已计入compiler进程wait4。

36编译/108native中72个功能00/ff、36个formal41，ABR/BRA/RAB/ARB/RBA/BAR原六排列及每round/arm/position全部核对。两个case整个sourcehash/oracle/vectors与原linear a30协议相等，再核实际640源文件SHA。独立重算全部251seed四步算术recurrence，用100000=398×251+102聚合mod1000003，108次实际stdin/stdout/stderr均与模型相等。各case各arm六轮ELF仅一个SHA；A/B各自materialIR去唯一定义verified-fingerprint后跨轮相等。跨arm不要求同码，实际输出与运行门分别验。

## 构建与实现身份

A/B flat buildbindings的source/tree/binary/成功release日志SHA均核对。B flat receipt1f6fd80b428a2d6f256691b532db1866e9275e25925f53fd57628327ad12c602绑定完整构建33c3634cf52ae203eb30e9594e8d9ed0179f3d66a24e5fcba1ca7b45f8ddf695，原四attempt api-tests/proof-tests/no-native/release的command、exit及stdout/stderr全部逐SHA读取。API测试在多个test group中合计64 passed/0failed/0ignored，proof合计6 passed；不误把64当成必须单组出现。no-native/release实际exit0，已有warnings原样保留，没有重新跑测试。

所有工具字节与协议相等；ZIL/Clang receipt与版本为23.1.4，Rust-vV明确1.99.1、bundledLLVM23.1.1，不称完全相同LLVM patch实现。显式LLVM DSO路径/解析路径与72146000字节、SHA2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250均核对。协议记录LD_LIBRARY_PATH，冻结driver在screen开始/结束计时外核相同DSO/loader环境；本审没有读取运行中/proc/maps，文件身份与实际API版本不冒充独立loaded-map抓取。A/B/R选compiler的对称计时外预核合同保持，B计时中不含旧wrapper首1GB身份扫描，全部actual API检查仍在compiler内。

## 原生两阶段与独立依赖图

24次语言Verify；A96份当前unit preflight与LLVM before/after；B12原模型、12分区输入模型、96当前unit preflight。B每次首object与独占外部current receipt完全相同、后续exact8objects。原预算proof为当前typedComplete，canonical owner唯一、原Required=Allowed及成本界均核。共96份prelink/1收据、192个prelink before/after验证标记、96份postlink/object/1实际检查收据；不存在旧external opt/dis/summary/HASH或plugin96hook，不能沿用那些旧计数。

独立从各unit actual definitions/declarations/direct_calls/intrinsics重建全图，使用定点可达而非作者DFS或validate_candidate_receipt，重算跨owner Required。每轮N64 source/copycost35777/31360、根Allowed504、Required56；N256144449/125440、2016、Required224，其他unit Required空。十二请求合计1680个私有导入、1680个direct uses、84个donor modules，与原许可、imports名单及owner计数相等。实际声明/调用仅限canonical、getchar/putchar及当前LLVMGetIntrinsicID名单，无llvm.*泛放行。

所有actual prelink均thinlto-pre-link<O2>/ordinary-no-summary、当前structured预检与before/after verify；post均manual-prelink<O2>|budgeted-direct-imports=private|thinlto<O2>|merge-functions=after|aliases=false，x86-64/PIC/defaultcode model、current-prelinked-bitcode输入，parse/optimize/emit正计时且两处opt/emit字段相等。缓存关闭、summary_index=false、official_thinlto_importer=false；24个ZIL报告commands均只有pinclang普通-O2 target链接与8objects，无flto/plugin/ThinLTO flags。

本路线未保存供外部解码的每份bitcode或hook前后文本，本审**不宣称独立LLVM反汇编、逐指令全文等价或外部Use multiplicity重算**。外部独立性来自对实际sealed C API facts的全图/owner/预算/Required/import/私有名/正Use数复算；native逐Use、parse/checksealed/trim/link/private/完整postlink校验依赖精确source非作者审和实际API收据，不能把JSON当Rust接受的授权capability。目标集合direct_calls不足以还原重复Use，报告保留这个证据边界。

## 本轮完整编译

单位ms，完整compiler进程start至wait4；每cell六正式请求。所有wall/CPU/RSS数组、median/max及冻结sample p95在JSON，n6不支持稳定尾分位声称。

| case | arm | wall mean | CPU mean | RSS mean KiB |
| --- | --- | ---: | ---: | ---: |
| n64/base | A | 1166.245886167 | 2074.276333333 | 291088.000 |
| n64/base | B | 1428.279647000 | 2110.976166667 | 291088.000 |
| n64/base | R | 831.410007167 | 1089.388333333 | 291088.000 |
| n256/base | A | 4456.924360000 | 7888.215833333 | 415993.333 |
| n256/base | B | 4867.913322500 | 8314.393333333 | 464198.667 |
| n256/base | R | 2655.821940167 | 4057.016166667 | 291088.000 |

| case | compile B/A wall | compile B/A CPU | R/B wall | result |
| --- | ---: | ---: | ---: | --- |
| n64/base | 1.2246814020 | 1.0176928371 | 0.5821058985 | NoGo |
| n256/base | 1.0922135826 | 1.0540271094 | 0.5455770808 | NoGo |

## 实际运行质量

| case | A mean ms | B mean ms | R mean ms | B/A≤1.10 | Rust/B≥.90 |
| --- | ---: | ---: | ---: | ---: | ---: |
| n64/base | 98.638607167 | 97.854975833 | 409.319054500 | 0.9920555312 | 4.1829150844 |
| n256/base | 325.831385833 | 342.743356333 | 1558.183156333 | 1.0519040560 | 4.5462096567 |

N256 B比A运行wall慢5.1904056%，仍在原10%门内，未隐去。全部native formal wall原始样本如下：

- n64/base A：100.064955, 97.019326, 100.997387, 99.581182, 95.515025, 98.653768
- n64/base B：90.409346, 112.771371, 93.994051, 97.232405, 101.617837, 91.104845
- n64/base R：368.884591, 391.809524, 382.575199, 472.310926, 411.610792, 428.723295
- n256/base A：332.440944, 309.697308, 330.295618, 321.996237, 340.974536, 319.583672
- n256/base B：338.369765, 362.703439, 347.392381, 323.195046, 335.011398, 349.788109
- n256/base R：1556.857920, 1494.945051, 1510.350651, 1537.573684, 1612.159642, 1637.211990

wait4 RSS为高水位且有launcher/fork floor，不当整个进程树同时峰值或各arm实际内存等同证据。

## 决策边界

只确认本版纯base两case质量Pass、完整compile NoGo。下一full default<O2> canonical producer及空actualRequired直接emit是新source/newformat/新冻结对照，不能覆盖a299数据、不能用Allowed是否为空偷换actualRequired，也不能提前推算收益。原八case、地址/global、多点/并发及产品权重门均未由本版证明。

独立JSON SHA256：`bc5daf832531fc26c358ff8a94c17d983e4a80178067e7240b11630c2f2d54b3`。
