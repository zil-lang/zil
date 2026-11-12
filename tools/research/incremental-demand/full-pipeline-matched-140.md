# Frozen 140997 三方测量

结果仅属于 `140997e032df26c31d94fe029ac86c7b42560182` / tree `832ab8bf0e10f5e3038b9872499e35be21e16bef`，候选二进制 SHA256 `2769bd9bd5d8ccbb83ba0aa8fea464a42429be5415ba128b36c4bb74f7af05c5`。后续 packed publication 与 epoch 恢复修改不在本结果内。六轮 wall/CPU/RSS 数值、父请求协议、实际分区代表映射及复用摘要见 [JSON](full-pipeline-matched-140.json)；全部 540 行原始记录、完整对象哈希与 nm 符号表由其中精确路径/哈希指针定位。

两个模式各 270/270 行有效：144 个测量请求、126 个具名父请求。O2 验证 2,808 次本机执行、351 份内容；78 对成功的 main/candidate 请求（含父请求）具有相同 executable 和 LLVM 文本 SHA256。原始证据仍在 local out/runs，待 v2 archive；JSON 记录其路径与哈希，暂没有外部归档链接。

表为六轮 wall 中位数，单位 ms；顺序均为 main / candidate / Rust。

| 场景 | check | O2 |
|---|---:|---:|
| cold | 978.9 / 1534.8 / 1144.4 | 6079.0 / 6995.0 / 5973.3 |
| unchanged | 88.6 / 3.2 / 13.4 | 518.6 / 111.7 / 28.0 |
| private | 708.0 / 274.6 / 299.3 | 1706.2 / 888.5 / 514.5 |
| multi | 679.8 / 233.4 / 309.3 | 1753.8 / 1069.2 / 1107.7 |
| broad | 799.6 / 1255.8 / 1225.3 | 5472.8 / 6696.3 / 5677.9 |
| restore | 112.1 / 119.6 / 238.7 | 615.0 / 553.6 / 565.2 |
| error | 440.9 / 435.6 / 306.7 | 372.4 / 325.0 / 300.3 |
| recovery | 709.0 / 412.5 / 311.5 | 1807.7 / 1202.0 / 569.2 |

候选对 main 的私有/多处编辑有收益，但 cold、broad 在两个模式均更慢；check restore 也稍慢。对 Rust，O2 unchanged/private/error/recovery 均更慢，recovery 为 1202.0 对 569.2 ms。六轮完整值保留，不以中位数隐藏波动，也不据此宣称统计显著或普遍领先。

请求采用 CGU16、O2、jobs1；Rust 为 1.99.1，固定 commit `58cf8f9c35b355199727627d07ae34a4e2846d67`、LLVM 23.1.1，Zil 为 LLVM API/clang 23.1.4。Rust 使用每请求新 Cargo 进程（`--offline --locked`）；Zil 为 resident 请求且 wall 不含启动。每块三方顺序平衡轮换；每个非 cold 测量前执行并保留具名父版本，恢复不清缓存。check/O2 分开窗口，无 profiling、无 OS cache flush。

O2 成功请求实际均有 16 个 current objects，分区内容并不相同。Zil private/multi/broad/restore/recovery 的 miss 分别为 1/2/16/0/1；Rust 物理沿用观测分别为 15/13/0/15/15 个对象，不可当作相同语义的缓存计数。Rust 错误请求仍保留前一成功会话的 16 个对象；Zil 错误没有当前 codegen inventory。JSON 以代表性的对象 SHA、实际 nm 成员映射及紧凑变更统计说明分区，并区分当前与历史会话；完整符号表保留在 raw。相同内容本身不能证明跳过了工作。

实际重复尚未消除：private 仍执行 5/425 个 Types CFG、21 个 BodyKey；虽然只 lower 1 个机器函数、独立检查 1 个函数证明。error 后 recovery 仍执行 404/425 CFG、421 BodyKey，证明父历史加速器 epoch 恢复缺口真实存在。独立 Verify 的每次成功入口保留，复用的是当前校验绑定的私有收据，不能与 Rust 安全合同等同。

CPU 与内存只保留有边界的观测：Zil 为 root `/proc` CPU 差值及 response RSS，peak 是进程累计高水位；Rust CPU 为 wait4 调用树统计，sampled RSS 只覆盖 Cargo root，wait4 maxrss 不是同时进程树总和。不得据这些不同范围宣称跨语言内存优势。对象/符号采集、源写入及 native oracle 执行在编译 wall 外。

这是一组固定合成负载上的历史基线，不是全程最简、完整通用点对点编译或 Rust 等价的完成声明。
