# 请求级身份去重 screen：独立原始结果审核

## 结论

**固定两格运行质量 Pass，完整编译请求两格均 NoGo。** N64/N256 的编译 B/A wall 为3.0335296447/2.5076074520，CPU为2.1056227134/1.8153868611；两种成本均显著高于主线。不得从旧窗口扣除身份或诊断耗时后宣称净加速，也不以运行质量通过代替编译收益。

本次只读原始JSON/IR/收据和冻结源码，没有编译、LLVM/plugin/native执行或新增计时，没有修改任何 `.py/.cpp`。完整独立结果、每条原raw SHA、逐round/unit/import证据及全部计时数组保存在同目录 `result-review.json`。其审计状态明确为 `Complete-independently-verified-runtime-Pass-compile-NoGo`，不把验证完成的Pass误读为编译候选采纳。

## 固定范围与完整性

正式协议为 `out/next-optimization/private-identity-protocol-v1/protocol.json`，SHA `722a146dd2bee2cbf5e4f1550532bf4404829bfc8aed46a956925590e4989215`；raw位于 `out/next-optimization/private-identity-screen-v1/`。协议原字节等于raw副本，冻结Python/C++逐SHA核对。case对象、源码、oracle/vectors、源与build绑定、工具身份、CPU0,1/jobs2、100000循环、六排列与运行门均与private208旧协议一致；新compile_screen严格按ABR/BRA/RAB/ARB/RBA/BAR，两个case各六轮、每轮三arm独立cold编译。

实际共36次正式完整编译、108次native，其中72功能输入00/ff、36正式输入41；每次新编译的产物都运行三输入，没有额外warmup行或缺失round。对每行case/round/position/arm顺序、validated、raw-metrics、process-attempt及JSONL逐项核等。**930份process-attempt全exit0、reaped且无timeout/exception/residual descendants**，未发现failure JSON，execution manifest为Complete/completed=true。原始 **6125个文件**在首次读取、核心统计复算及详细hook复核后完整重新枚举，逐SHA完全相等。

独立从四步算术recurrence重算两case全部251个seed值和cycle sum，再按100000=398×251+102聚合mod1000003；未调用作者fixture或oracle生成函数。108份实际stdin/stdout/stderr逐项符合三输入独立数值模型。每case每arm六轮ELF各只有一个SHA；A/B各自去唯一verified-fingerprint后的materialIR也跨六轮相等。跨arm允许代码不同，未用旧同ELF候选门冒充运行质量证明。

## 首次完整身份校验确在每个 B 计时请求内

十二份 `round-*/compile-B/wrapper/request-tool-identities.json`均绑定本轮protocol SHA、**该次**current机器receipt路径/SHA，full_validate_completed=true；每份精确记录 **10次完整工具哈希、1,031,047,064字节**。十个绝对调用路径集合恰为九工具role加插件构建compiler；clang/clang++别名仍按原first-full-validation分别读字节，未将首次校验也提前去重。各digest符合固定tool identity，记录的所有target/link stat字段与当前实际文件重新核等；源码/IR/BC/proof未注册进固定工具快速路径。

其时间边界由冻结源码链与实际独占目录/收据共同确定：compile_screen拒绝预存wrapper目录，随后外层 `run→measured`启动并等待B编译器；新wrapper子进程执行validate_request，先清空进程内旧fastpath，再持该请求独占flock，在不存在stamp时完整validate并计数，完成后才发布stamp。后续八个wrapper沿用这次fresh stamp并核stat，仍执行小文件/当前proof验证。首次完整验证因此处于B编译器进程start→wait4的完整计时内，不由runner在循环前的一次prevalidate替代；每次B新目录和machine context也不能复用上一请求的stamp。

本数据没有每次hash独立耗时或物理磁盘I/O计数，1.031GB是源码与收据一致的逻辑读取量；不把它直接等同耗时归因。独立读取工具SHA只用于审核，不是新性能测量。

## 完整验证和实际私有导入

| 真实证据 | 独立计数 |
| --- | ---: |
| 当前语言Verify | 24 |
| A当前unit preflight和LLVM before/after | 96 |
| B原模型 / 分区输入模型 / unit preflight | 12 / 12 / 96 |
| 实际prelink summary及module HASH | 96 |
| 外部LLVM verify：每B请求32次 | 384 |
| plugin实际hook / before-after verifyModule | 96 / 192 |
| hook独占输出文件 | 384 |
| 实际AE→私有Internal导入 / direct uses | 1680 / 1680 |

对十二个B请求均独立提取整函数input/prelink/.3文本、重算block+instruction原费用和固定点跨owner闭包，不调用作者visibility parser/DFS。每轮N64原source/copycost35777/31360、根Allowed504、实际Required=Added=AE56；N256分别144449/125440、2016、224。六轮各自原Required=Allowed、唯一owner、实际Required⊆Allowed和总费用界成立；另外84个非根unit无AE/私有导入。原允许helpers在prelink已inline，不要求以每个原符号再次单独导入。

所有真实summary/HASH、保留link inputs、imports、.3/.4验证命令及成功metrics逐项核对；每请求fresh native-cache为空，exact八个actual hooks，无命中绕过。actual link仍只有一次force-import-all和固定plugin路径，完整O2/target/PIC/merge/aliases/jobs/threads不变，所有工具进程在对应外层B owned group。每格getchar无ownerbody的已知诊断原样保存且与固定允许项相同，不被忽略成“任何stderr都允许”。

每份manifest/currentRust/prelink/plugin/protocol/source_filename、started/完成receipt、前后IR SHA均独立核等。hook-before AE集合等于实际保存.3，改写名单恰为Required，固定私有名没有collision/残留canonical。独立对全文符号引用和实际直接call目标作Counter双计数（不是作者check_hooks），1680个imports各恰一处directuse；后IR实际均Internal，visibility/DLL均0、dso_local false→true。将批准private token逆回canonical、仅规范化对应header的available_externally/internal/dso_local后，整个module文本逐字节相等；没有放过指令/CFG/属性/metadata差异。全部零导入unit也检查。

## 完整编译统计

单位ms；每cell六正式完整请求，保留全部长尾。JSON另存六点wall/CPU/RSS、median/max和冻结规则的sample p95；n=6不构成稳定尾分位估计。

| case | arm | wall均值 | CPU均值 | RSS均值KiB |
| --- | --- | ---: | ---: | ---: |
| N64 | A | 1309.322091 | 2371.032000 | 295584 |
| N64 | B | 3971.8673775 | 4992.4988333 | 295584 |
| N64 | R | 811.358654833 | 1091.410000 | 295584 |
| N256 | A | 4324.369303667 | 7694.4478333 | 414585.333 |
| N256 | B | 10843.820691167 | 13968.399500 | 384396 |
| N256 | R | 2648.3200385 | 4055.3716667 | 295584 |

| case | B/A wall | B/A CPU | R/B wall | 编译结论 |
| --- | ---: | ---: | ---: | --- |
| N64 | 3.0335296447 | 2.1056227134 | 0.2042763712 | NoGo |
| N256 | 2.5076074520 | 1.8153868611 | 0.2442238869 | NoGo |

N64 wall原始六点：A1483.972070/1049.875917/1510.206841/1331.827176/1402.828603/1077.221939；B4061.050552/3835.419735/4554.812261/3988.586062/3892.994390/3498.341265；R924.319710/651.104220/829.056096/749.739134/939.257495/774.675274。N256：A5272.298068/4451.529267/4548.223717/3746.928072/4060.726040/3866.510658；B11750.482768/11247.939224/10249.574894/10893.011020/10540.592251/10381.323990；R2923.540417/3026.104778/2489.442966/2523.909817/2490.614248/2436.308005。

## 运行质量复核

| case | A wall均值ms | B wall均值ms | R wall均值ms | B/A≤1.10 | Rust/B≥.90 |
| --- | ---: | ---: | ---: | ---: | ---: |
| N64 | 103.828015333 | 73.590773500 | 400.685336333 | 0.7087756928 | 5.4447768012 |
| N256 | 340.942922500 | 312.134965000 | 1576.735762000 | 0.9155050432 | 5.0514551037 |

两格真实运行质量门均Pass。所有formal native RSS均295584KiB，属于launcher/fork floor，不能据此推断真实内存相同或整个进程树同时峰值。两格纯base没有多点编辑、并发或原八case质量覆盖；本次没有用户场景频率加权或产品总体门。

## 决策边界

本轮完整NoGo保持原样。下一新ROOT拟将hook每个import重复扫描整个IR改为线性Counter，这只是已定位的实现工作，不能先归因全部剩余成本、从本轮时间扣减或预告收益；必须另有冻结版本及真实完整请求结果。本轮已经证明请求级大工具身份读取减少到一次仍不足以获得编译收益；旧private运行Pass和本轮运行Pass都不能替代这一结论。没有源改动、降O2、删Verify/preflight/LLVM验证、调预算/阈值或挑case挽救结果。
