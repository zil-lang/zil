# 当前大项目：加权耗时、磁盘与合入覆盖

指定九窗现均有 completion：**7 窗完整通过、2 窗 full-ELF 首败，共 864 行**。两失败窗均保留 `comparisons=[]`，不补双 block 中位数。Source5105 另为容量证据，不入任何速度/磁盘优势表。

共同 main 为 `9170bb4210554392e36eb7528596612596ebef80`，host-release control-01 binary SHA256 `08fcf4bc3b31590068585b63f4f4bc18def552234b9c57295b9a4b0fb4f199f0`。工具源 `ad81486b976634e71bf1c61544d0fd28a783fbae`，冻结 harness 的七文件 SHA 与原始窗口完全一致；每窗完整 source/tree/binary/build receipt/tool/runtime/fixture/model 绑定及 raw 元数据 SHA 见 JSON。这里的 host control 是 main 宿主 release 构建控制，不声称窗口另有 CPU0 affinity 证据。

计算：每个实际请求先取两 block wall 中位数，再对原 workflow 求和。私改频次是实际 8/32 次；贡献=频次×各 epoch 中位数的平均。正节省=参照−候选；负值必须保留。不把毫秒倍率或跨候选总和当用户收益，不将这些合成频次当工业频率。

## stock 30c0009：合入候选

source `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb`；binary `7826460c9609fc558ac7aac461cfa329ca8f809d11df6e480de30df9881abb6d`；LLVM DSO `2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250`。本节仅实际 main / 该候选 / Rust，未列旧候选。

| 窗口 | 请求/行/质量 | main 总秒 | 候选总秒 | Rust 总秒 | 对 main 节省秒 | 对 Rust 节省秒 |
|---|---|---:|---:|---:|---:|---:|
| stock425-02-check | 17 请求，102/102 通过 | 11.737 | 11.895 | 8.076 | -0.158 | -3.819 |
| stock425-02-o2 | 17 请求，102/102 通过 | 32.310 | 31.341 | 25.800 | +0.970 | -5.540 |
| stock-large2500-02-check | 17 请求，102/102 通过 | 3.439 | 3.623 | 4.146 | -0.184 | +0.523 |
| stock-large2500-02-o2 | 17 请求，102/102 通过 | 13.476 | 12.023 | 9.167 | +1.453 | -2.855 |
| stock-large1000-02-o2 | 3/102 行，full-ELF 首败 | — | — | — | — | — |

冷、broad、私改与其余请求的绝对贡献（下列节省均依次为对 main / 对 Rust，单位秒）：

| 窗口 | cold | broad | 私改频次×每次平均节省 | 私改合计 | 其余合计 |
|---|---:|---:|---|---:|---:|
| stock425-02-check | -0.405 / -0.455 | -0.220 / +0.175 | 8 × (+0.084 / -0.446) | +0.671 / -3.571 | -0.203 / +0.033 |
| stock425-02-o2 | +0.812 / +0.639 | +0.119 / +0.856 | 8 × (-0.073 / -1.038) | -0.585 / -8.301 | +0.623 / +1.266 |
| stock-large2500-02-check | +0.180 / +0.473 | +0.160 / +0.068 | 8 × (-0.087 / -0.085) | -0.694 / -0.681 | +0.169 / +0.663 |
| stock-large2500-02-o2 | +0.071 / -0.883 | +0.312 / -0.895 | 8 × (-0.064 / -0.192) | -0.509 / -1.533 | +1.579 / +0.456 |

磁盘顺序 main / 本节候选 / Rust。每次实际 snapshot 先合计一臂全部 unique-inode 类别再取 max，非 sum(max(category))；MiB=2²⁰ bytes。

| 窗口 | 物理 peak MiB | 逻辑 peak MiB | 文件数 peak | 私改数 | cold→最后私改物理增长 MiB（两 block 中位数） |
|---|---:|---:|---:|---:|---:|
| stock425-02-check | 82.26 / 82.26 / 7.25 | 82.19 / 82.19 / 7.15 | 15 / 15 / 23 | 8 | 51.44 / 51.44 / 1.92 |
| stock425-02-o2 | 97.56 / 86.55 / 18.97 | 97.40 / 86.39 / 18.78 | 69 / 69 / 125 | 8 | 51.88 / 51.88 / 7.88 |
| stock-large2500-02-check | 22.49 / 22.49 / 29.30 | 22.44 / 22.44 / 29.22 | 15 / 15 / 24 | 8 | 14.56 / 14.56 / 7.55 |
| stock-large2500-02-o2 | 26.91 / 26.23 / 42.51 | 26.73 / 26.05 / 42.33 | 69 / 69 / 126 | 8 | 14.88 / 14.88 / 11.53 |

stock 暖请求回退保留如下（正数是额外耗时，不因 cold 或总量获益删除；private 为整段频次贡献）：

| 窗口 / 请求组 | 对 main 额外秒 | 对 Rust 额外秒 |
|---|---:|---:|
| stock425-02-check / unchanged ×2 | +0.002 | +0.167 |
| stock425-02-check / private edits ×8 | -0.671 | +3.571 |
| stock425-02-check / positions ×1 | +0.242 | +0.224 |
| stock425-02-check / restore ×1 | +0.056 | -1.059 |
| stock425-02-check / warm-restart ×1 | +0.040 | +0.096 |
| stock425-02-o2 / unchanged ×2 | +0.314 | +1.310 |
| stock425-02-o2 / private edits ×8 | +0.585 | +8.301 |
| stock425-02-o2 / positions ×1 | -0.071 | +0.381 |
| stock425-02-o2 / restore ×1 | +0.097 | -4.803 |
| stock425-02-o2 / warm-restart ×1 | -1.177 | +0.523 |
| stock-large2500-02-check / unchanged ×2 | -0.002 | +0.022 |
| stock-large2500-02-check / private edits ×8 | +0.694 | +0.681 |
| stock-large2500-02-check / warm-restart ×1 | +0.002 | +0.011 |
| stock-large2500-02-o2 / unchanged ×2 | -0.249 | +0.439 |
| stock-large2500-02-o2 / private edits ×8 | +0.509 | +1.533 |
| stock-large2500-02-o2 / positions ×1 | +0.010 | +0.019 |
| stock-large2500-02-o2 / warm-restart ×1 | -0.587 | +0.204 |

## research 1de00f63：研究候选

source `1de00f63f80cf4534ec995dc51e106005b270ddb`；binary `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7`；LLVM DSO `785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2`。本节仅实际 main / 该候选 / Rust，未列旧候选。

| 窗口 | 请求/行/质量 | main 总秒 | 候选总秒 | Rust 总秒 | 对 main 节省秒 | 对 Rust 节省秒 |
|---|---|---:|---:|---:|---:|---:|
| large2500-02-check | 17 请求，102/102 通过 | 2.811 | 1.642 | 4.035 | +1.169 | +2.393 |
| large2500-02-o2 | 17 请求，102/102 通过 | 11.436 | 8.572 | 7.833 | +2.864 | -0.739 |
| large1000-edits-02-check | 41 请求，246/246 通过 | 7.001 | 2.491 | 5.146 | +4.510 | +2.655 |
| large1000-edits-02-o2 | 3/246 行，full-ELF 首败 | — | — | — | — | — |

冷、broad、私改与其余请求的绝对贡献（下列节省均依次为对 main / 对 Rust，单位秒）：

| 窗口 | cold | broad | 私改频次×每次平均节省 | 私改合计 | 其余合计 |
|---|---:|---:|---|---:|---:|
| large2500-02-check | -0.098 / +0.689 | +0.022 / +0.232 | 8 × (+0.144 / +0.104) | +1.153 / +0.836 | +0.091 / +0.637 |
| large2500-02-o2 | -0.282 / -1.431 | -0.044 / -1.183 | 8 × (+0.247 / +0.091) | +1.976 / +0.724 | +1.214 / +1.151 |
| large1000-edits-02-check | -0.004 / +0.130 | -0.030 / +0.168 | 32 × (+0.140 / +0.065) | +4.472 / +2.096 | +0.071 / +0.261 |

磁盘顺序 main / 本节候选 / Rust。每次实际 snapshot 先合计一臂全部 unique-inode 类别再取 max，非 sum(max(category))；MiB=2²⁰ bytes。

| 窗口 | 物理 peak MiB | 逻辑 peak MiB | 文件数 peak | 私改数 | cold→最后私改物理增长 MiB（两 block 中位数） |
|---|---:|---:|---:|---:|---:|
| large2500-02-check | 22.49 / 12.75 / 29.30 | 22.44 / 12.67 / 29.22 | 15 / 26 / 24 | 8 | 14.56 / 5.19 / 7.55 |
| large2500-02-o2 | 26.91 / 16.37 / 42.51 | 26.73 / 16.17 / 42.33 | 69 / 70 / 126 | 8 | 14.88 / 5.41 / 11.53 |
| large1000-edits-02-check | 53.74 / 13.62 / 12.16 | 53.69 / 13.44 / 12.06 | 39 / 74 / 24 | 32 | 46.62 / 8.64 / 3.09 |

## 32 次私改与全部 ELF 首败

研究 1000-short check：32 次私改合计 main/候选/Rust `6.128 / 1.656 / 3.751` 秒，候选整段节省 +4.472s（main）、+2.096s（Rust）；不以单次毫秒倍率替代此量。

| block | 32-edit 物理增长 MiB main/候选/Rust | 端点平均 KiB/次 main/候选/Rust |
|---:|---:|---:|
| 0 | 46.62 / 8.64 / 3.09 | 1492.00 / 276.38 / 98.88 |
| 1 | 46.62 / 8.64 / 3.09 | 1492.00 / 276.38 / 98.88 |

`large1000-edits-02-o2` source `1de00f63f80cf4534ec995dc51e106005b270ddb`：仅 block0 cold 三行，三臂各 12 次、合计 36 native oracle 通过，semantics/CGU16/aggregate exports 为 true；**完整 ELF equality=false，窗口 valid=false**。3/246 行，未执行 243 行；`comparisons=[]`，没有完整 workflow/私改增长/速度优势结论。原始 cold 单样本仅作诊断：baseline=1.836s, candidate=1.804s, rust=0.996s。

`stock-large1000-02-o2` source `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb`：仅 block0 cold 三行，三臂各 12 次、合计 36 native oracle 通过，semantics/CGU16/aggregate exports 为 true；**完整 ELF equality=false，窗口 valid=false**。3/102 行，未执行 99 行；`comparisons=[]`，没有完整 workflow/私改增长/速度优势结论。原始 cold 单样本仅作诊断：baseline=2.671s, candidate=2.403s, rust=1.787s。

## Source5105：仅容量，不是倍率

原 short5000 fixture 实际为 **5105 声明 / 5103 函数定义、100 计算模块、102 源文件、843875 bytes**。原 fixture SHA256 `1c0d646573d16a65c96e80da7af0ce0ee6a49ca0e2b047229695e98155c07e03` 已读回核对；source file SHA 表、原始 harness/helper hashes、binding 与逐行原报告在 JSON。

当前研究候选完整 Source→Verify→native 容量成功，Rust 的 O2 成功；Rust check 是明确标注的历史复用收据。**main 冷/热 check/O2 全部仍在 Expand 因默认预算 ResourceLimit 失败**，不是通过，不是计时参照，不据其快失败计算倍率或存储优势。此案例没有完整 matched completion，完全排除于上方双 block workflow 表，`formal_ratio=null`、`comparisons=[]`。

## 解释边界与合入评估

仍仅 synthetic scalar：wide imports 下 branchy425/short1000/simple2500，实际未用定义 error、位置、broad、error/recovery、restore 和 warm restart；未裁真实源/闭包或提高预算。本批没有 current-source chain/diamond 的新窗口，不能挪用旧候选的那两窗归到当前源。

尚缺工业 generic/trait/单态化、async、proc macro/build scripts、跨 crate 依赖和公共 API 变化、自动 64-reset、工业频率分布、10000 函数完整成功窗口。单 Cargo package 模块导入不等于 cross-crate build；显式 warm restart 不等于 64-reset。当前仍非全面 Rust/工业目标完成。

合入评估应同时保留 stock 暖回退、两候选所有 full-ELF 首败以及仍慢于 Rust 的绝对总秒；容量提升不能覆盖机器质量拒绝。只在实际完整 valid 窗内比较，不跨 source 或历史窗口求统一“提升率”。

当前成功 ZIL 请求均核了独立 Verify；只有 O2 成功窗声称执行 native oracle/实际 CGU16/完整 ELF 门。每请求计时不含归档、disk/native 检查；resident startup 单记，Rust 新 Cargo/rustc 启动含在 wall。两 block、OS cache 未清，磁盘扫描可能预热。

磁盘只统计 semanticcache、objects、program/program.ll 与 Rust target incremental/deps/binary/other；跨 roots hardlink 去重继承真实快照。不含 source/context/runtime/evidence、SDK、Cargo home、归档或 resident 原生产品内存。峰值是请求边界观察值，非构建瞬时峰值。每峰的 raw row/moment/category、逐 epoch 增长与 warm-restart delta 均在 JSON。

## 逐窗身份

- `stock425-02-check`：source `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb` / tree `9d15a0b3e70e4e0a15138d54eb7ffb7956ad7f82` / binary `7826460c9609fc558ac7aac461cfa329ca8f809d11df6e480de30df9881abb6d`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `297ba93e50d2baabcc430fa2a505a91db0b4d6fd16b2d9deb265027fa537ca99`；manifest `98aafe743118d1cbadb8b7c93cfdf9f27cba8ebcf3c14e1a6a7806c0a738a59f`；rows `3a4d053bc1aab29e13fc34224d7a5f6294dd802c847f0bc92fb8f19e0b05ce9d`。
- `stock425-02-o2`：source `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb` / tree `9d15a0b3e70e4e0a15138d54eb7ffb7956ad7f82` / binary `7826460c9609fc558ac7aac461cfa329ca8f809d11df6e480de30df9881abb6d`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `297ba93e50d2baabcc430fa2a505a91db0b4d6fd16b2d9deb265027fa537ca99`；manifest `3b047845c0644a405ce492350e5ee76b4787bcd27e03f3d9b9d8a32efd899360`；rows `78c1358e9793f8ecb0b0f702124ec5ec5f02145bd981451b5b0465b7ac35dcd3`。
- `large2500-02-check`：source `1de00f63f80cf4534ec995dc51e106005b270ddb` / tree `850ff626b6e9cc227d29b2cbe823fa769df52c8c` / binary `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `86a29ee5313874fd66b7c0f4d2c07b5cb0337cfe08f4362e07e272ffbfd58875`；manifest `3172e04f2e8cb4305afec0ff171c78ff697955f6ac3a508cd538ef683d0f4ccb`；rows `b0912352a079e2ccc6d1cd5152aa62f9f37b9795d8f631494e11a503f3afd747`。
- `large2500-02-o2`：source `1de00f63f80cf4534ec995dc51e106005b270ddb` / tree `850ff626b6e9cc227d29b2cbe823fa769df52c8c` / binary `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `86a29ee5313874fd66b7c0f4d2c07b5cb0337cfe08f4362e07e272ffbfd58875`；manifest `ff62dec4cafc278657c1dcc7213b797740eeb58b7321db07fbc91a4cbd0fc744`；rows `631558fe549c983e513c569c775f9f428687d9e9956a7fd014ab0b4ec399180f`。
- `large1000-edits-02-check`：source `1de00f63f80cf4534ec995dc51e106005b270ddb` / tree `850ff626b6e9cc227d29b2cbe823fa769df52c8c` / binary `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `68b26549638bf52f6e319e3ac38c33b08a82f0d082c3489741203147a82a5cd9`；manifest `28976eab1413f5aa22656b0923b34d4a6b641fb3029306af05c72eab3612eec0`；rows `2dc10c973fc414c4c47559ea05672adbdd7c3eaa42f0d2a3c6ed7051e9fb0671`。
- `large1000-edits-02-o2`：source `1de00f63f80cf4534ec995dc51e106005b270ddb` / tree `850ff626b6e9cc227d29b2cbe823fa769df52c8c` / binary `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `68b26549638bf52f6e319e3ac38c33b08a82f0d082c3489741203147a82a5cd9`；manifest `216daeef5b0bc91123af64e1ab6c5a7133f414952c0ba6a4a207aa7b0fd4dc71`；rows `efbda8108744950a1fe94e85ec00ab7e3ea68a40b9e1a3c6014c5c5407975ac2`。
- `stock-large2500-02-check`：source `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb` / tree `9d15a0b3e70e4e0a15138d54eb7ffb7956ad7f82` / binary `7826460c9609fc558ac7aac461cfa329ca8f809d11df6e480de30df9881abb6d`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `86a29ee5313874fd66b7c0f4d2c07b5cb0337cfe08f4362e07e272ffbfd58875`；manifest `a5b845781e62569ef6d9a45c8f9bc26bb72c99fe252de571bfa14386783f156d`；rows `a4fd7ca0a6a35cc9035c6ec4b9692e79cf2e7313c8ffc1c25a9137e80971f05d`。
- `stock-large2500-02-o2`：source `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb` / tree `9d15a0b3e70e4e0a15138d54eb7ffb7956ad7f82` / binary `7826460c9609fc558ac7aac461cfa329ca8f809d11df6e480de30df9881abb6d`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `86a29ee5313874fd66b7c0f4d2c07b5cb0337cfe08f4362e07e272ffbfd58875`；manifest `6eadeb2ef05a84681c37765c0fe1fc2c435ba99f3c95469ed6d8d17e7ebe4cc6`；rows `004c5256b2bc6e82229ac8c1fe5e72e257960d42c13fadeb3a55a8f99ef77839`。
- `stock-large1000-02-o2`：source `30c0009ff94b3476c15f4afe2ba5d067f7f78cbb` / tree `9d15a0b3e70e4e0a15138d54eb7ffb7956ad7f82` / binary `7826460c9609fc558ac7aac461cfa329ca8f809d11df6e480de30df9881abb6d`；tool `ad81486b976634e71bf1c61544d0fd28a783fbae`；fixture `68b26549638bf52f6e319e3ac38c33b08a82f0d082c3489741203147a82a5cd9`；manifest `f4e8d392c09f24eaf13e2edcd08e387deacb5864cf4546019dcd5a9177d75ae8`；rows `7b47e9fe2eb58d94cb5b0d9f8825e34749c857c051e9f7410aa7ae7158f7f4f6`。

JSON 包含每窗完整 manifest、source/binary build receipt、归档工具 SHA、原始 source fixture SHA 全表、每请求 source 表与模型/LLVM/object/layout 引用。只重哈希绑定元数据/工具副本/源 fixture，不重跑 compiler/native、完整 blob 哈希或质量门；没有操作 Git 或修改冻结源码/工具。
