# Linear hook计数 screen：独立原始结果复核

## 结论

**固定两case运行质量Pass，完整编译两格均NoGo。** 本轮N256 B运行wall比A慢3.2866901%，仍在原10%退化门内，保留该不利结果。请求级身份核验与linear是两个独立session，不是成对性能实验；本报告不计算跨session“27%提速”，不从完整请求扣除校验/日志成本，不给生产采用或多点并发收益结论。

本审仅读固定源码与既有raw，未编译、运行LLVM/plugin/native或新增计时，未改任何.py/.cpp。两个纯base、100000循环、独立251seed数学模型、六排列及原门均未放宽。

## 身份与完整性

protocol位于`out/next-optimization/private-linear-protocol-v1/protocol.json`，SHA `a30ffe86586c77849a613be636ece4f58fa3bfcebbc0eec3191cba333ce4e72c`；raw为`out/next-optimization/private-linear-screen-v1/`。原字节等于raw副本；全部冻结scripts/current工具SHA核对。cases对象、每源hash、oracle/vectors、source/build/plugin/tool身份、CPU0,1/jobs2、100000、ABR/BRA/RAB/ARB/RBA/BAR顺序与identity722a协议逐项相同。B仍为exporter2549加原pin插件，linear只更改研究Python的重复计数路径，没有换O2或插件算法。

完整36编译、108native=72功能00/ff+36formal41。全部6125原raw文件首次读取、核心审核后及详细hook/native审核后重新枚举SHA全等；930process-attempt逐份与raw-metrics一致，exit0/reaped，无timeout/exception/residual，未发现failure记录。manifest为Complete/completed=true。JSON保留每条raw路径/SHA、全部round/unit/import和样本数组，无删样/复测替换。

独立重算所有251seed的四步算术recurrence及周期和，再用100000=398×251+102聚合mod1000003，三输入输出与冻结oracle及108次actualIO全等；不调用作者生成器。case/round/arm/position逐行符合六顺序。各case各arm六轮ELF各仅一个SHA；A/B各自materialIR去唯一定义verified-fingerprint后跨轮一致，跨arm代码不同由真实运行门处理。

## 当前身份与全部验证

12个B请求各有本请求protocol/currentmachine路径+SHA的fresh工具stamp，完整核验10次、1,031,047,064逻辑字节；全部工具digest/current文件大小匹配。首fullvalidate在外层measured启动的B进程内由首wrapper完成，后续wrapper用同request stamp/stat；runner循环前的身份校验不替代它。没有单项hash耗时或物理I/O样本，不把逻辑字节直接折成耗时。

当前24次语言Verify；A96个unit当前preflight/LLVM before-after；B12次原模型、12次分区输入模型及96个unit当前preflight；96份actualsummary/HASH，384次外部LLVM verify（每B32次）、96个actualhook和192次plugin内部verifyModule。每次fresh nativecache，exact8hook，无缓存绕过。actual target/PIC/完整O2/after merge/aliasesfalse/jobs/threads/force-import-all一次/plugin身份保持。getchar已知无ownerbody诊断保留。

独立整函数读取和固定点跨owner闭包重算全部12请求：N64 source/copycost35777/31360、根Allowed504，actualRequired=Added=AE56；N256144449/125440、2016、224。六轮各自canonical owner唯一、原Required=Allowed、actualRequired⊆Allowed及费用界成立，另外84个非根unit无AE。没有要求已prelink-inline helpers再次逐符号导入。

96hook×4文件完整，manifest/request/protocol/plugin/currentRust/prelink/sourcefilename/started与完成字段及actualIR SHA一致。独立Counter全文符号引用减唯一定义、实际call目标双计数，1680imports各一directuse；未调用作者check_hooks。after固定私有Internal名不碰撞、不残留本module canonical；逆精确private名字，仅规范化选中header的AE/Internal/dso_local，整个module全文逐字节相等，零AE unit同样检查。body/CFG/attrs/metadata未获得额外豁免。

## 本轮完整编译

单位ms，均为实际完整compiler进程start至wait4，六正式样本；全部wall/CPU/RSS数组、median/max/sample p95在JSON，n6不支持稳定尾分位声称。

| case | arm | wall mean | CPU mean | RSS mean KiB |
| --- | --- | ---: | ---: | ---: |
| n64/base | A | 1165.508022000 | 2072.047333333 | 295580.000 |
| n64/base | B | 3446.844230500 | 4362.174500000 | 295580.000 |
| n64/base | R | 731.090907167 | 1045.557000000 | 295580.000 |
| n256/base | A | 4284.156129000 | 7671.044166667 | 414378.667 |
| n256/base | B | 7957.408558500 | 11042.704500000 | 384029.333 |
| n256/base | R | 2536.086977333 | 3915.369166667 | 295580.000 |

| case | compile B/A wall | compile B/A CPU | R/B wall | decision |
| --- | ---: | ---: | ---: | --- |
| n64/base | 2.9573749519 | 2.1052484805 | 0.2121044231 | NoGo |
| n256/base | 1.8574039598 | 1.4395308201 | 0.3187076494 | NoGo |

## 本轮运行质量

| case | A wall mean ms | B wall mean ms | R wall mean ms | B/A≤1.10 | R/B≥.90 |
| --- | ---: | ---: | ---: | ---: | ---: |
| n64/base | 105.526712333 | 78.438784333 | 397.199182833 | 0.7433073825 | 5.0638110497 |
| n256/base | 328.673192167 | 339.475661333 | 1592.487619000 | 1.0328669007 | 4.6910214793 |

所有formal native wall样本（ms），长尾完整保留：

- n64/base A：99.197035, 97.017691, 104.353494, 105.145365, 102.016876, 125.429813
- n64/base B：70.950989, 99.721650, 79.672987, 75.269661, 72.105774, 72.911645
- n64/base R：381.788941, 384.133747, 394.865356, 408.343319, 402.867549, 411.196185
- n256/base A：330.588900, 324.131113, 323.260331, 359.306118, 320.334041, 314.418650
- n256/base B：318.053041, 337.906417, 320.195213, 382.568237, 307.292415, 370.838645
- n256/base R：1742.808707, 1583.366462, 1507.605616, 1603.821808, 1560.118886, 1557.204235

所有formal native RSS为295580KiB，属于launcher/fork floor；不据此推断实际内存相同或进程树同时峰值。

## 决策范围

只可确认linear删除重复全IR计数这一实现机制及本轮完整请求的上述真实比率；跨session旧identity与本轮原始结果均保留，不合并挑格、不偷换权重。编译NoGo不因质量Pass消失；两纯base不构成原八case、地址/global、多文件编辑、并发或产品总体门证明。下一原生C API/prelink实验必须另有源码与协议冻结/真实数据，不用本轮结果代报其成功。

独立JSON SHA256：`a065ceda1913cf0dbf666b685e8f431719a96a8fdc907d7d2873c2e723539699`。
