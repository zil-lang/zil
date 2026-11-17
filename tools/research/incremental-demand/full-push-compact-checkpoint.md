# 原生重启首改整链结果

源码 `15f858c4ce183fedee9eb3a46082b4ee74ee404b`；完整缓存读取、恢复和发布链同批验收。

NoGo: first-edit target unmet; ordinaryCLI C1796.207/Rust1238.061ms; all6pairsfaster=False; comprehensive goal remains open.

| 场景 | ZIL秒 | Rust秒 |
| --- | ---: | ---: |
| 驻留请求 cold | 3.270923 | 6.154449 |
| 驻留请求 unchanged | 0.109766 | 0.022445 |
| 驻留请求 private-001 | 1.739086 | 1.445658 |
| 驻留请求 recovery | 1.989997 | 5.898568 |
| 普通CLI完整进程 cold | 2.715802 | 6.352213 |
| 普通CLI完整进程 private-001 | 1.796207 | 1.238061 |

原30请求/10功能格、另12运行构建/4运行格；普通CLI六个交替区块，共36次构建。用户盘量完整历史采样峰 ZIL 30.0–30.0 MB，Rust 61.5–76.7 MB；每个时点全局inode去重后再取峰，含实际缓存和最终产物，不含SDK/宿主构建/研究输出。

strict、IR95、backend402、Driver265、native publication4、fmt、release和Astra源码独审通过。未改Expand/Types/Analyze/Verify的原通过收据按差异复用。

机器原始缓存各段共持有完整已校验文件；借用解码保持remaining node边界；待发布产品按原字节流散列并流式写出，删除重复整份复制和聚合。原schema、完整hash/EOF、16MiB逻辑上限、费用和manifest最后原子发布保持。

旧约1.5秒为驻留请求；普通CLI包含启动和退出。window29驻留1.776/Rust1.484秒、CLI1.800/Rust1.074秒；window30驻留1.599/Rust1.386秒、CLI1.890/Rust1.142秒，均未达标；不能把时钟差异当作回退的全部解释。旧window26–30负结果均保留，跨窗口差额不冒称因果收益。

完整结果：[current.json](native-direct-edit-chain-20261116/current.json)；历史：[window30](native-direct-edit-chain-20261116/window30.json)。实际main超限、工业多crate/proc-macro/build.rs、分钟/GB规模及全面超越目标仍开放；未合main。

同版六CLI样本阶段中位数：后端635ms（机器产生238ms、类变换168ms、对象119ms），Types195ms、Expand113ms、Verify74ms；各字段中位数不可相加。分区11ms、诊断文本8ms；一次未复现大停顿的系统调用诊断中fsync12ms、页回收13ms，不能据此归因数百毫秒尖峰，也未据此改持久化政策。
