# 大项目：实际频次加权耗时与磁盘覆盖

仅汇总指定八窗：7 完整通过、1 首败，实际 861 行。只读后处理；没有构建、执行 native、重新测量或更改冻结工具。完整身份、逐请求样本、源文件 SHA 表、pair 收据和磁盘峰值见同名 JSON。

每窗先对**每个实际请求、每臂的两个 block**取 wall 中位数，再按原序列求和。正节省 = 参照 − 候选；负数表示候选多用时间。私改按实际 8/32 次计权，不把它们混成一个无频次的毫秒比率，也不假定这些频次代表工业用户。重复整个序列 n 次的净节省为表中秒数 × n；不跨不同候选合计。

## stock 候选：aaa72a6b8

候选 source `aaa72a6b84c097844d7484280992a1c7e4696ab1`，binary SHA256 `09e58eefb883427935a18a9f9d181961121ccb8982d3641986b7172de379c116`；main source `9170bb4210554392e36eb7528596612596ebef80`，binary `8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e`。候选 LLVM DSO `2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250`。本表只含该候选与其对应的实际 main/Rust。

| 窗口 | 实际请求数 | 行/质量 | main 总秒 | 候选总秒 | Rust 总秒 | 对 main 节省秒 | 对 Rust 节省秒 |
|---|---:|---|---:|---:|---:|---:|---:|
| stock425-01-check | 17 | 102/102 通过 | 11.721 | 11.584 | 8.503 | +0.137 | -3.081 |
| stock425-01-o2 | 17 | 102/102 通过 | 34.275 | 43.668 | 25.011 | -9.393 | -18.657 |

按绝对秒数分解（每次私改数值为各 epoch 请求中位数的平均；乘实际次数后恰好回到总贡献）：

| 窗口 | cold 节省秒 main/Rust | 私改频次 | 私改每次平均节省秒 main/Rust | 私改合计节省秒 main/Rust | 其余请求合计节省秒 main/Rust |
|---|---:|---:|---:|---:|---:|
| stock425-01-check | +0.526 / +0.711 | 8 | -0.050 / -0.546 | -0.400 / -4.364 | +0.011 / +0.572 |
| stock425-01-o2 | +1.173 / +0.461 | 8 | -0.698 / -1.897 | -5.584 / -15.175 | -4.981 / -3.942 |

磁盘数值顺序均为 **main / 本表候选 / Rust**，MiB=2²⁰ bytes。先逐快照合计该臂各类别的 unique-inode charge，再求峰值；下表不是“各类别最大值相加”。

| 窗口 | 全臂物理峰值 MiB（两 block 最大） | 全臂逻辑峰值 MiB | 文件数峰值 | 私改数 | cold→最后私改物理增长 MiB（两 block 中位数） |
|---|---:|---:|---:|---:|---:|
| stock425-01-check | 82.26 / 82.26 / 7.25 | 82.19 / 82.19 / 7.15 | 15 / 15 / 23 | 8 | 51.44 / 51.44 / 1.92 |
| stock425-01-o2 | 97.56 / 86.63 / 18.97 | 97.40 / 86.47 / 18.78 | 69 / 69 / 125 | 8 | 51.88 / 51.88 / 7.88 |

## 研究候选：8ce584f81

候选 source `8ce584f81a405b8c5aa69e555b80ede276f07143`，binary SHA256 `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`；main source `9170bb4210554392e36eb7528596612596ebef80`，binary `8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e`。候选 LLVM DSO `785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2`。本表只含该候选与其对应的实际 main/Rust。

| 窗口 | 实际请求数 | 行/质量 | main 总秒 | 候选总秒 | Rust 总秒 | 对 main 节省秒 | 对 Rust 节省秒 |
|---|---:|---|---:|---:|---:|---:|---:|
| large1000-edits-01-check | 41 | 246/246 通过 | 6.728 | 2.203 | 6.569 | +4.526 | +4.366 |
| large1000-edits-01-o2 | 未完成（计划 41） | 3/246，full-ELF 首败 | — | — | — | — | — |
| large2500-01-check | 17 | 102/102 通过 | 3.185 | 1.841 | 3.379 | +1.344 | +1.538 |
| large2500-01-o2 | 17 | 102/102 通过 | 10.587 | 14.268 | 7.545 | -3.681 | -6.723 |
| chain1000-01-check | 17 | 102/102 通过 | 1.244 | 0.970 | 2.035 | +0.275 | +1.065 |
| diamond1000-01-o2 | 17 | 102/102 通过 | 4.323 | 5.186 | 5.785 | -0.863 | +0.599 |

按绝对秒数分解（每次私改数值为各 epoch 请求中位数的平均；乘实际次数后恰好回到总贡献）：

| 窗口 | cold 节省秒 main/Rust | 私改频次 | 私改每次平均节省秒 main/Rust | 私改合计节省秒 main/Rust | 其余请求合计节省秒 main/Rust |
|---|---:|---:|---:|---:|---:|
| large1000-edits-01-check | -0.026 / +0.398 | 32 | +0.133 / +0.097 | +4.255 / +3.101 | +0.297 / +0.867 |
| large2500-01-check | -0.196 / +0.254 | 8 | +0.165 / +0.088 | +1.317 / +0.700 | +0.224 / +0.584 |
| large2500-01-o2 | -2.799 / -3.934 | 8 | +0.170 / +0.037 | +1.359 / +0.295 | -2.241 / -3.085 |
| chain1000-01-check | -0.041 / +0.133 | 8 | +0.039 / +0.058 | +0.308 / +0.464 | +0.008 / +0.468 |
| diamond1000-01-o2 | -0.753 / -1.035 | 8 | +0.041 / +0.216 | +0.330 / +1.728 | -0.439 / -0.094 |

磁盘数值顺序均为 **main / 本表候选 / Rust**，MiB=2²⁰ bytes。先逐快照合计该臂各类别的 unique-inode charge，再求峰值；下表不是“各类别最大值相加”。

| 窗口 | 全臂物理峰值 MiB（两 block 最大） | 全臂逻辑峰值 MiB | 文件数峰值 | 私改数 | cold→最后私改物理增长 MiB（两 block 中位数） |
|---|---:|---:|---:|---:|---:|
| large1000-edits-01-check | 53.74 / 13.62 / 12.16 | 53.69 / 13.44 / 12.06 | 39 / 74 / 24 | 32 | 46.62 / 8.64 / 3.09 |
| large2500-01-check | 22.49 / 12.75 / 29.30 | 22.44 / 12.67 / 29.22 | 15 / 26 / 24 | 8 | 14.56 / 5.19 / 7.55 |
| large2500-01-o2 | 26.91 / 16.37 / 42.51 | 26.73 / 16.17 / 42.33 | 69 / 70 / 126 | 8 | 14.88 / 5.41 / 11.53 |
| chain1000-01-check | 8.79 / 5.18 / 12.01 | 8.74 / 5.10 / 11.90 | 15 / 26 / 24 | 8 | 5.66 / 2.16 / 3.03 |
| diamond1000-01-o2 | 10.63 / 6.69 / 17.50 | 10.43 / 6.50 / 17.26 | 69 / 70 / 126 | 8 | 5.84 / 2.28 / 4.59 |

## 32 次私改与 native1000 首败

1000-short **check** 的 32 次私改合计 main/研究候选/Rust 为 5.583 / 1.328 / 4.429 秒；候选节省 +4.255 秒（对 main）、+3.101 秒（对 Rust）。下列为实际冷状态之后的缓存增长，包含可能的重写/压缩负斜率；不是持有原生产品的内存大小。

| block | 32 次私改物理增长 MiB main/候选/Rust | 端点平均 KiB/次 main/候选/Rust |
|---:|---:|---:|
| 0 | 46.62 / 8.64 / 3.09 | 1492.00 / 276.38 / 98.88 |
| 1 | 46.62 / 8.64 / 3.09 | 1492.00 / 276.38 / 98.88 |

1000-short **O2** 首轮仅 cold 的 block0 三行：三臂各 12 次、共 36 次原生输入/输出/退出 oracle 通过；`semantics_equal`、`physical_zil_units_valid`、`aggregate_zil_function_exports_equal` 均 true，但 **`executable_equal=false`，整体 valid=false**。已执行 3 / 计划 246 行，缺 243 行；不能算 pass，不能补成双 block 中位数、不能推断 32 次 O2 节省或磁盘增长。

诊断性原始 cold 单样本：baseline 1.487s，candidate 3.605s，rust 0.980s。这些不是通过质量门的性能结论，也不以较小失败磁盘充作优势。完整失败 pair 与原错误路径见 JSON。

## 覆盖与缺口

本轮只有 synthetic scalar：425 声明/423 函数定义的 branchy16、1005 声明/1003 定义的 short1、2505 声明/2503 定义的 simple 直线表达式。大例含 100 个计算模块及 100 个未用定义，error 在未用定义中注入未知名字；所有模块/定义保持完整，未裁闭包。chain 为 0→1→…→99；bounded diamond 为 0→[1,2]、1/2→3、3 以后单链，尾部真实执行两次，无指数运行。

覆盖 cold、unchanged、实际 8/32 次私改、位置变化、broad、error/recovery、restore 与显式 warm restart。chain 这里只有 check，diamond 这里只有 O2；1000-short O2 不完整。显式 warm restart 不是 64-reset 测试。

尚未覆盖工业 Rust 编译痛点与真实频率分布：大型泛型/trait/单态化、async/state machine、proc macro/build scripts、跨 crate 依赖及公共 API/签名变化、64-reset 行为和 10000 函数默认预算成功。单 Cargo package 内模块 imports 不是跨 crate 构建；不能由这些合成窗口宣布全面或稳定领先 Rust。

成功 ZIL 请求均保留当前独立 Verify；成功 O2 窗另受原生 oracle、物理 CGU16 与完整 ELF 门约束。check/O2 分开；仅原生窗口声明三臂实际 CGU16/O2/jobs1，check 不声称执行了 codegen 或 native gate。ZIL resident 启动单记，Rust Cargo/rustc 启动包含在请求 wall；OS cache 未清，磁盘扫描可能预热。这里只汇总编译响应时间，未把 native oracle、归档、磁盘扫描或 resident 启动加进请求总秒。

磁盘只含 ZIL semanticcache/objects/program/program.ll 与 Rust target 的 incremental/deps/binary/other，硬链按原快照跨 roots 去重；不含源码、context/runtime/evidence 日志、SDK、Cargo home、归档或 resident 内存产品。峰值仅为请求前后边界观察峰值，非构建过程瞬时峰值；各类别取峰时刻完整值、逐 epoch 点及 warm-restart delta 在 JSON。

## 逐窗身份与归属

- `stock425-01-check`：candidate `aaa72a6b84c097844d7484280992a1c7e4696ab1` / tree `1ebf1963c56c197f004c4f333793922d039aa6ef` / binary `09e58eefb883427935a18a9f9d181961121ccb8982d3641986b7172de379c116`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；preset/shape/graph `small425/branchy/wide`；manifest SHA `81526ebcef76f47aefad515913f8bb0bf949b1b0d8fec4742a0c33cd6c5f3aa7`。
- `stock425-01-o2`：candidate `aaa72a6b84c097844d7484280992a1c7e4696ab1` / tree `1ebf1963c56c197f004c4f333793922d039aa6ef` / binary `09e58eefb883427935a18a9f9d181961121ccb8982d3641986b7172de379c116`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；preset/shape/graph `small425/branchy/wide`；manifest SHA `993bcd71f4759da572b0bf9c6c8bf6884ae9dedbb09c99a5027973f4ff7a1c76`。
- `large1000-edits-01-check`：candidate `8ce584f81a405b8c5aa69e555b80ede276f07143` / tree `35a051da13de600c71ddc9ecb76edbe82cf23c85` / binary `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；preset/shape/graph `1000/short/wide`；manifest SHA `cc0c3386a632773711eb3e1caf717f2813cbf55a1b24a38c7114c0174531ee25`。
- `large1000-edits-01-o2`：candidate `8ce584f81a405b8c5aa69e555b80ede276f07143` / tree `35a051da13de600c71ddc9ecb76edbe82cf23c85` / binary `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；preset/shape/graph `1000/short/wide`；manifest SHA `88d23dc87208e1de6053117ea9b9de8d24af8907e7cd2e2450f6750a551e8145`。
- `large2500-01-check`：candidate `8ce584f81a405b8c5aa69e555b80ede276f07143` / tree `35a051da13de600c71ddc9ecb76edbe82cf23c85` / binary `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；preset/shape/graph `2500/simple/wide`；manifest SHA `e617bbecc2c162f95be62f83d75ba8c11b8714683806eb417de59056a5ba771a`。
- `large2500-01-o2`：candidate `8ce584f81a405b8c5aa69e555b80ede276f07143` / tree `35a051da13de600c71ddc9ecb76edbe82cf23c85` / binary `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；preset/shape/graph `2500/simple/wide`；manifest SHA `f08770fd115edc0cdee4836b353b1fd8e09840d997d7ac2c12b7386fde25adbd`。
- `chain1000-01-check`：candidate `8ce584f81a405b8c5aa69e555b80ede276f07143` / tree `35a051da13de600c71ddc9ecb76edbe82cf23c85` / binary `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；preset/shape/graph `1000/simple/chain`；manifest SHA `22393f80748e2559b263b08918946c4b0dcd037bc0fb73b3e3e41b4d4bc9bea4`。
- `diamond1000-01-o2`：candidate `8ce584f81a405b8c5aa69e555b80ede276f07143` / tree `35a051da13de600c71ddc9ecb76edbe82cf23c85` / binary `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；preset/shape/graph `1000/simple/diamond`；manifest SHA `9195e401428ceaae0031d32d24e57e691e231012bf5b2929cb239181a53a982b`。

共同 main source `9170bb4210554392e36eb7528596612596ebef80` / binary `8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e`；Rust 1.99.1（实际 rustc/Cargo SHA、版本原文及 launcher 绑定在 JSON）。冻结 e005777aa 的新成本选择和之后的集成代码没有被这些窗口测量，不能据此归因其收益。这里只重哈希小型绑定元数据、工具副本、源 fixture/row 表，没有重新重哈希 compiler/SDK/ELF blobs 或重新执行质量验收。
