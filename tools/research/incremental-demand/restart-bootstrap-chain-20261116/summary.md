# 重启首次修改整链：8f06a9e 精简结果

被测源码 `8f06a9ebb635e64b49aef5affdcd7ab7e0c0e639`；binary SHA256 `1965c26f73a3a0bb2d42b3a43929c8207bd516a20e0dc9bb5a050fe7453f5936`。实际main `9170bb4210554392e36eb7528596612596ebef80`；Rust1.99.1，同O2/CGU16/compiler jobs1，两区块，check/native分开。

| 原生5105 | ZIL秒 | Rust秒 |
|---|---:|---:|
| 冷编译 | 3.973 | 6.901 |
| 无修改 | 0.039 | 0.026 |
| 重启构建 | 2.535 | 0.022 |
| 紧接首改 | 1.033 | 1.266 |
| 连续重启构建＋首改 | 3.568 | 1.287 |

原生六请求流程 10.955/Rust 15.472s；用户边界峰值 32.684/58.691MiB。36请求/12格/5运行质量通过。main5105默认ResourceLimit，不能算快成功。

check5000wide：全流程 6.832/Rust 6.832s，接近持平；冷 1.484/0.804s仍输。8私改 1.068/2.362s。用户边界峰值 13.602/58.348MiB，8改增长 1.035/15.141MiB。102请求/34格通过；保留冷编译和第7次私改负结果。

单独直接重启首改诊断：冷/无改后关闭进程，先修改，再启动首个请求，没有先跑无改预热。3.715s，257字节+EOF原生oracle通过；这是profile诊断，不与正式Rust窗口作比值。Analyze 1168.9ms、Codegen 1355.7ms（内含partition 447.4ms）；layout仍miss，Analyze5105个product查询仍执行。它更接近普通CLI每次启动后的首次修改，优先级高；常驻服务频率较低，没有用户频率遥测或百分比。

实现：真实当前相等scalar机器体在请求内共享，4857次复用/143个模板生产；新类拆分/合并按当前图局部适配与物化；只读CT scalar不复制AST；真实Preflight与sealed classifier合并遍历，原classifier节点域和正式费用独立保留。未引入持久Source→Machine权限，独立Verify/原默认预算/完整O2和原研究默认保持。

验收：Expand125（9c，子树未变）、backend329（439，子树未变）、driver230/native2/strict/fmt/release（8f）。两位非作者复核当前源码无阻塞。首次Vec模式、旧类调用图拒绝、夹具重复名、旧clone计数失败与修复有各自运行身份；未放松原拒绝断言。

全部正式138请求/46格/5运行质量完成，全面超Rust仍未实现。下一链直接处理重启首改Analyze全量product admission和空进程磁盘类布局恢复；不重复旧全矩阵，不打包大证据。旧480e/bee7/98fa/021997数字保留原版本归属。工业多crate/proc-macro/build.rs及分钟/GB覆盖仍缺。

完整命令/计数/关键阶段见[精简JSON](summary.json)，必要原始运行留本地out/runs/cache-restart-*03与cache-restart-direct-edit-diagnostic-01。SDK18/native-link恢复沿固定e85/V32；没有重建LLVM，没有合main。
