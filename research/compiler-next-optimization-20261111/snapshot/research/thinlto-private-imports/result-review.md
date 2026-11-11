# 私有导入正式两 case：独立原始结果复核

## 结论

**固定纯 base 两 case 的运行质量门 Pass，协议未失效；尚无编译加速或生产采用结论。** N64/N256 的 B/A 完整六正式样本均值比为 0.7387339795/0.9105647474，均满足 ≤1.10；Rust/B 为 5.1888733054/5.0376022735，均满足 ≥.90。全部54个原生输出均与独立数值模型相等。默认 ThinLTO 和严格 AE 完整导入的旧 NoGo 仍各自保留，不能拿独立窗口之间的比值变化当配对机制估计。

本审只读已存在 raw、Python/C++、协议和构建收据；没有编译、LLVM 工具、plugin/native 执行或新计时。没有修改任何 `.py/.cpp`，没有导入作者 `check_hooks` 或 `visibility` 辅助函数代替独立复核。独立结果与完整 raw SHA 列表保存于本目录 `result-review.json`。

## 固定身份与数据完整性

| 身份 | SHA/位置 |
| --- | --- |
| 正式 protocol | `out/next-optimization/private-protocol-v1/protocol.json`，SHA `208adf6f5a3e904182afdaf1ef1c6b30a173fb559400ab14d05b2f59e4803a44` |
| raw | `out/next-optimization/private-runtime-v1/` |
| 插件 source/tree | `1cc866a08bac100012e41726067711c94caf1899` / `910d281b288b9266f67c92ae9891b3b9290d16d2` |
| 插件 C++ | `a22e5004b5f99150ae197ffa88f406eafb2136c796d5d3cc651a8fc97cc99623` |
| 插件 SO | `585a84701033e21669a2465df4007150e1fddf36df1acdab3165c48a16734cf6` |
| 插件 build binding | `056e812dbb270f07c40abf9658fa6ef3007ed76bc8f5869941ed49322fdcf2a9` |
| raw summary | `3b12a25e3e354941c97ad28c920306ce576dda232b5be708606192fc9496d44c` |
| raw native rows | `7de288dc1e24019399169642bb296d78c5ba73c2deb14f287a1c5cac1aca2011` |
| raw execution manifest | `df36609dc2303761e197cf2615a7706fbd0f42e615668fd651a57eb23e6e4c45` |

原协议 bytes 等于 raw 内副本。A 仍为 main990/binary64e1ff43，B 为 exporter2549/binaryfdfe6cea 加独立插件，Rust仍1.99.1，LLVM工具仍23.1.4；逐工具现存字节、源码冻结、A/B buildbinding与成功日志绑定均核对。插件 source/so/buildbinding 再核 SHA，actual link 仅一次传入所 pin load-pass-plugin；prelink 不加载插件。源码与 oracle/六顺序/CPU/样本/门恒等 strict5b0ff 的静审结论见目录外 `thinlto-private-imports-python-review.md`，本次又逐 case 实际 sourcehash 核对。

全 raw **1260 个文件**独立首次哈希、核心复算后哈希、额外 hook 审查后重新枚举哈希均相等；JSON 保留每条 path/SHA。共 **196 份 process-attempt**，全部 exit0、reaped=true、无 timeout/exception/residual descendants，逐份等于同目录 raw-metrics。未发现 failure JSON；execution manifest 为 completed=true/Pass。六份 compile validated metrics 与实际 attempt 全等，输出程序 SHA 在编译后及每次 native 行均核对。没有省略首失败、过滤 invalid 或复测替换样本。

本审第一次额外 hook 检查曾误读 manifest 的已清理临时原 `.o` 路径而停止，未写审计 Pass；修正为按原 link 的 `original→saved` 映射核保留副本 SHA。它是审计读取路径错误，不是正式实验失败，也没有重新执行任何实验步骤。

## 实际验证与输出

| 证据 | 独立计数 |
| --- | ---: |
| clean 编译 | 6 |
| native 总数 / formal / warmup / functional | 54 / 36 / 6 / 12 |
| 当前语言 Verify | 4 |
| A 当前 unit preflight 与 LLVM before/after | 16 |
| B 原模型 / 折叠后分区输入模型 / unit preflight | 2 / 2 / 16 |
| actual prelink summary + module HASH | 16 |
| LLVM input/prelink verify | 32 |
| 实际 .3 verify / 实际 .4 verify | 16 / 16 |
| plugin hook unit / 独占文件 | 16 / 64 |

从四步源 recurrence 独立重算每个 leaf 的八个函数，遍历全部251个seed，再用 `100000=398×251+102` 的周期和与余项聚合得到 acc mod1000003；不调用 fixture/oracle 生成器。所得全部251值及cycle sum等于冻结oracle，00/41/ff四字节小端输出等于冻结vectors，并逐行核实际 stdin/stdout/stderr。每 case27行的 kind/round/arm/position 与 ABR/BRA/RAB/ARB/RBA/BAR 六排列及原warmup/functional顺序全等。

独立核 ELF、material IR 原字节及删除唯一定义的 verified-fingerprint 行后的 SHA；A/B 代码允许不同，其质量由各自产物实际执行和同数值oracle验证，未套用旧同ELF实验门。

## 原预算和实际私有导入

独立用整函数正则提取 input/prelink/.3 的函数体，并以定点可达性重算跨owner闭包；不用作者 parser/DFS 或 receipt true 值替代算法。原函数费用按 block+instruction 重算，canonical owner 唯一，原 Required=Allowed、总copy界和实际 prelink Required⊆Allowed 均成立。

| case | 原 source cost | 原 copy cost | 根 unit 原 Allowed | actual .3 Required/Added/AE | hook 私有 Internal / direct uses |
| --- | ---: | ---: | ---: | ---: | ---: |
| N64 base | 35777 | 31360 | 504 | 56 / 56 / 56 | 56 / 56 |
| N256 base | 144449 | 125440 | 2016 | 224 / 224 / 224 | 224 / 224 |

两格仅 unit0 有以上导入，另外十四个 unit 的 Required、新增AE、私有改写均为0。原函数名费用分别3136/12544，原完整允许闭包费用仍31360/125440，不能把预链接后IR大小当原费用或声称复制预算减小。实际 .3 的全部非own定义恰为Required/AE，名单未越原Allowed；内部helpers已在prelink内联，不要求每个原helper符号单独导入。每格 imports文件只有根unit非空，列出七个source modules，另外七个为空。

每份 hook 的manifest/request/protocol/plugin/Rust/prelink identity、source_filename/index/owner、before/after实际 SHA和started/完成收据均独立核对；临时bitcode原路径与保留link输入的相同SHA对应。hook-before AE 名单等于真实保存 .3 的 AE，imports条目恰按canonical排序覆盖全部AE。对每个名字独立重算全文去注释符号引用减唯一定义header，并另数直接call目标；二者等于记录 direct_uses，本轮280个private imports各恰一处直调。

hook-after全部对应固定 `zil.import.<unit>.<canonical>` Internal定义，旧canonical在本module完全不留，新定义集合恰等于own集合减AE加private。实际 visibility/DLL前后均0，dso_local false→true。将精确private tokens逆回canonical，仅在这些函数header规范化AE/Internal/dso_local三个词后，**整个module文本逐字节相等**；没有允许body、CFG、属性、metadata或其他函数差异。零AE unit同样全文核等。此处独立检查未调用作者check_hooks。

## 完整运行计时

单位ms；均为完整native launch、stdin至wait4，六formal全部保留，未去长尾。n=6不足以声称稳定尾分位数。

| case | arm | wall mean | CPU mean | wall 六个原始值 |
| --- | --- | ---: | ---: | --- |
| N64 | A | 96.236436667 | 95.058666667 | 99.515735, 93.410294, 98.035568, 94.059324, 94.234487, 98.163212 |
| N64 | B | 71.093125833 | 70.711166667 | 73.106324, 71.012497, 71.856644, 71.326614, 70.221368, 69.035308 |
| N64 | R | 368.893222833 | 367.694166667 | 375.855324, 374.576906, 368.984847, 364.270017, 365.375054, 364.297189 |
| N256 | A | 334.862908667 | 331.697833333 | 328.380492, 329.345606, 330.689442, 343.189321, 335.791315, 341.781276 |
| N256 | B | 304.914359833 | 302.759833333 | 302.850560, 312.268605, 315.499938, 302.935532, 292.813628, 303.117896 |
| N256 | R | 1536.037272333 | 1528.144666667 | 1592.605886, 1526.538429, 1541.207383, 1502.962003, 1484.470089, 1568.439844 |

| case | B/A≤1.10 | Rust/B≥.90 | 结果 |
| --- | ---: | ---: | --- |
| N64 | 0.7387339795173214 | 5.18887330539025 | Pass |
| N256 | 0.9105647473690641 | 5.037602273546361 | Pass |

所有formal RSS均295500KiB，属于wait4 launcher/fork floor；不能据相同值证明实际memory footprint相同，也不是整进程树聚合峰值。完整CPU/RSS原始数组、median/max/p95按冻结规则保存在独立JSON，p95只是六点sample max。

## 完整编译诊断不隐去

| case | arm | gross wall ms | gross CPU ms | RSS KiB |
| --- | --- | ---: | ---: | ---: |
| N64 | A | 1255.691878 | 2224.391 | 295500 |
| N64 | B | 8285.685575 | 14240.383 | 295500 |
| N64 | R | 826.768316 | 1062.860 | 295500 |
| N256 | A | 4955.839589 | 8773.109 | 414472 |
| N256 | B | 14630.546176 | 22469.956 | 384316 |
| N256 | R | 2453.920127 | 3660.671 | 295500 |

这些完整cold请求包含proof、hash、verify、disassembly和hook前后IR落盘，当前B请求明显更贵；不能删掉上述开销后把本轮说成编译加速，也不能以runtime两格通过宣布产品采用。现有证据仅说明：在固定预算和当前纯base范围内，实际完整导入加私有Internal权限后，真实两case同时通过既定运行质量门。其他case、地址/全局语义及去诊断后完整请求/多点编辑收益仍未由本轮证明。
