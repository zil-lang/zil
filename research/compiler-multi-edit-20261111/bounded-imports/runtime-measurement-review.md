# 1679运行质量门：冻结脚本独立审核

结论：**Runtime测量器静态审核Pass，可由root先prepare冻结协议并执行固定运行门。** 本审核没有prepare、import/运行新脚本、编译或原生采样；协议尚未生成，执行结果仍须后续按实际协议SHA256与raw复核。此Pass不表示1679已通过运行质量门。

| 文件 | 实际SHA256 |
|---|---|
| runtime_fixture.py | `cddd3ae272f5a316a383cb17c472e08823e7af63cbd52304e3160693820f4154` |
| prepare_runtime.py | `d0b6adf39d403fd80d6d8dfd11891d18735a10a7b39586f6f6c9c290ffc05808` |
| runtime_gate.py | `702829ad5358c8482a6f0a25d2456f0f71969edf97713c81724b47e777911568` |
| 共享screen.py | `8cc25d67e4e575f4fb4b0a2f0035c82e1904d96c4309f52d608b3b6bb77612c3` |
| 共享fixture.py | `b4d588232df9b0d41d8877adcf99c712a90cb8f931adcf991dce85510ca7f314` |

实算hash与最终声明一致。shared validate_zil、artifact_identity、zil_command、frequency及validate_clang_pin已实读；不借用旧a60同码结论支持新的跨arm代码差异。

## 独立oracle和整数边界

runtime source保留原64/256动态corpus的所有leaf字节和main中的纯函数prefix，prepare逐字比较reference snapshot；只换完整程序的运行循环。8case固定N64/256×k0/1/16/64，没有从采样选择case或调整cost32。

main先读取真实一字节input，运行100000次，seed=(input+i)%251，将value(seed)累加并每次模1000003。Python独立求值器不调用source generator的model/body_parameters、不读取机器输出；直接从整数case参数求251个value。100000=398×251+102，任意input起点的完整周期和相同，剩余102项按(input+i)%251索引，故包含input255→seed4的情况；oracle再对总和模1000003，与每次累加取模等价。Python大整数承担周期乘积，不能把其可能超过i32误解为原生中间值溢出。

原生值域完整：seed0..250，每leaf y经模保持0..250；乘法系数最大12、offset最大37、edit最大2，乘加上界3039。N256时value最大255×8×250+3=510003；acc最大1000002，acc+value最多1510005，远低于2147483647；N64更小。input+i最大255+99999=100254，i结束值100000也安全。所有右移/putchar字节非负且不超过255。三个input0/65/255独立预测精确四字节输出；EOF单列返回2，不会进入测量语义。

## 固定次序、计数和真实门

8case每个A/B/R各clean编译一次，共24编译。每case先额外功能输入00/ff各3arm共6运行，再每arm一轮41预热共3运行，再六种三arm排列共18正式运行，总27×8=216；其中正式144、预热24、额外功能48。

六正式排列ABR/BRA/RAB/ARB/RBA/BAR穷尽全部排列，每arm在每位置恰两次。runner同时校验六排列唯一性、成员和每位置计数；全部六个正式观测进入算术均值。输入、轮次和次序固定，不以去长尾或追加轮数求通过。

每case分别要求B/A均值<=1.10、Rust/B均值>=0.90，且全部原生输出有效；8case全部通过才生成Pass。后一个比例等价于候选速度至少为Rust的90%，不能反写成B/R>=.90。geomean仅附加描述，不覆盖任一case失败。运行失败、timeout和输出错误保留，不能过滤出成功子集；缺失CPU/RSS使相应统计不可用，未悄悄丢掉样本。

## 编译、工具和失败证据

A/B均clean、无object-cache、完整LLVM API O2/incremental-o2/after、8CGU/2jobs；每份ZIL report要求Success、当前Verify1、实际N文件、全部unit当前完整preflight及合法LLVM前后核验证据。A/B ELF/materialIR分类记录，允许分区导致不同机器码，但输出和真实运行门必须各自成立。Rust显式O2、overflow checks、panic abort、8CGU，同一个固定clang linker；源到二进制身份由root提供构建收据支撑，protocol如实记录source claims。

prepare冻结原reference协议哈希、每个源文件、全部脚本、A/B/R二进制与clang完整字节。run重算所有身份，要求当前ZIL_CLANG解析到同一已pin clang，并将--clang显式传给ZIL、同路径传给Rust -Clinker，关闭前稿的工具漂移缺口。每次native前还重哈希已编译program。

measured用独立进程组、wait4直接回收，finally始终先落process-attempt.json，包含command、exit/exception、timeout、wall、已回收CPU/RSS和cleanup；调用方先落raw-metrics，再解析compile JSON或native输出。启动失败、timeout、异常和取消均留证并仅清理本次进程组。外层fatal收据保存phase/case/arm/round/position、完成计数和traceback，未完成工作不能计Pass。程序返回错误但能继续时仍保存正式观测并使case失败；原始失败不被后续code identity或geomean抵消。

观测频率为observed-uncontrolled，RSS是wait4最大单进程值，wall包含启动、一字节输入与结束，均如实声明。有限8case/6正式样本只是预注册质量门，不构成任意程序的运行质量保证，也不替代原有更广父门。

## 编译/并发前置关系

本报告仅先放行runtime准备/执行。compile screen已看到实际runtime-summary核验与各arm own-clean逻辑，另行完成最终审核。当前concurrent初稿只写execution_prerequisite文字、未实际核验运行门，已明确阻断其执行并交负责作者修复；此问题不要求改变已冻结runtime脚本，也不妨碍runtime先行。
