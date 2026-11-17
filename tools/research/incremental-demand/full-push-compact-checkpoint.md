# 原生重启首改整链结果

源码 `5fe48bda9bfd4d541d26580f12e2681eb03a9659`，二进制 `4e472f597524e44766680ef1ff1f9617f18a5f870721f59e7208c496afe9f594`。输入、Expand、Types、缓存发布同批实施并统一验收。

NoGo: first-edit target unmet; ordinaryCLI C1767.598/Rust1217.055ms; all6pairsfaster=False; comprehensive goal remains open.

| 场景 | ZIL秒 | Rust秒 |
| --- | ---: | ---: |
| 驻留请求 cold | 3.281434 | 6.656113 |
| 驻留请求 unchanged | 0.059020 | 0.018883 |
| 驻留请求 private-001 | 1.583330 | 1.222044 |
| 驻留请求 recovery | 1.711590 | 5.842942 |
| 普通CLI完整进程 cold | 2.655648 | 6.357395 |
| 普通CLI完整进程 private-001 | 1.767598 | 1.217055 |

5105函数/102文件，实际main/候选/Rust，O2/CGU16/compiler jobs1。原30请求/10功能格、另12运行构建/4运行格及六CLI区块36构建均完成；功能与运行质量通过。CLI首改5负1胜，目标未完成，不合main。

CLI同一时点全局inode去重峰值：候选27.62–27.66MB/Rust61.15MB；完整编辑历史约29.17–29.20MB/Rust61.55–76.73MB。含实际缓存及最终产物，不含SDK/宿主target/研究文件。

已实施：ASCII/最长词法分派与单次UTF-8扫描；恢复后的Expand按原阶段汇总费用，省逐函数包装与多轮回放；Types共享实际解码owner、延迟身份包装并消费当前推断投影；canonical输出与encoding一次绑定；CT与前端共享保存的完整源字节，恢复原收据后再走原准入，原逻辑限额保持。

strict、Syntax79、Expand153、Types136、backend402、Driver267、native publication4、fmt、release、Astra独审通过。独立Verify、原预算/O2及研究开关默认关闭保持。首次strict机械问题、保留类型名fixture错误和零doctest统计误报已保留并修正；一行fixture变化后仅补受影响检查和release。

六CLI阶段独立中位数：Expand109ms、Types163ms、Verify74ms、后端648ms（机器产生231ms、类变换161ms、对象125ms）；嵌套/分别取中位数字段不可相加。单次同口径阶段诊断的输入/解析65.6ms、实际parser58.7ms；该诊断类变换出现664ms高值，不能用单样本替代六组结论，也未查明全部波动成因。Rust确实以incremental=1执行冷编译和首改，LLVM嵌套时间不重复相加。

window31普通CLI为1.796/Rust1.238秒；本轮1.768/1.217秒，仅约28ms差额，不能宣称明显整链改善或归因于单一补丁。此前1.5秒驻留口径和全部负结果仍保留。冷编译领先不等于首改领先：尚未将全机器恢复/变换裁成仅受影响工作，不能继续把局部命中当作目标达成。

精简结果：[current.json](native-direct-edit-chain-20261116/current.json)，前次：[window31](native-direct-edit-chain-20261116/window31.json)。实际main仍超限；工业多crate/proc-macro/build.rs、分钟/GB规模和全面超越目标未验成。无LLVM重建或全量证据打包。
