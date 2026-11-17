# 原生重启首改整链结果

源码 `7bc2bc0d8fad62092cd9a1addf2be48821837998`；生产源码 `164475f92d215ad53507d2d9783308894fe66af5`。完整 A/B/Driver 链同批验收。

NoGo: first-edit target unmet; ordinaryCLI C1890.425/Rust1141.811ms; all6pairsfaster=False; comprehensive goal remains open.

| 场景 | ZIL秒 | Rust秒 |
| --- | ---: | ---: |
| 驻留首请求 cold | 3.463705 | 6.582309 |
| 驻留首请求 unchanged | 0.058530 | 0.025412 |
| 驻留首请求 private-001 | 1.599002 | 1.386397 |
| 驻留首请求 recovery | 1.967163 | 5.778311 |
| 普通CLI完整进程 cold | 3.209168 | 6.678362 |
| 普通CLI完整进程 private-001 | 1.890425 | 1.141811 |

原30请求/10功能格、另12运行构建/4运行格；普通CLI六个交替区块，共36次构建。用户盘量完整历史采样峰 ZIL 30.0–30.0 MB，Rust 61.6–76.7 MB；每个时点全局inode去重后再取峰，含实际缓存和最终产物，不含SDK/宿主构建/研究输出。

strict、Expand151、Types134、backend399、Driver265、native publication4、fmt、release和Astra源码独审通过。164475的两处测试lint失败与修正保留；编译检查通过。

恢复后的共享函数体贯穿Analyze/机器类消费者；完整当前比较仅按实际共享体执行，逐行头部、费用和当前Verify保留。闭图裁剪直接校验当前完整机器图，删除临时linkage克隆与还原；完整当前文件比较消费已验checksum快照，原边界和原子发布保持。

旧约1.5秒为驻留请求；普通CLI包含启动和退出。window29驻留1.776/Rust1.484秒、CLI1.800/Rust1.074秒仍未达标；不能把时钟差异当作回退的全部解释。旧window28/29负结果均保留，跨窗口差额不冒称因果收益。

完整结果：[current.json](native-direct-edit-chain-20261116/current.json)；历史：[window29](native-direct-edit-chain-20261116/window29.json)。实际main超限、工业多crate/proc-macro/build.rs、分钟/GB规模及全面超越目标仍开放；未合main。
