# Final reuse chain 13：最终证据独立复核

**裁决：身份、实际门收据、原始样本、质量计数与有限窗口数值结论接受；outer checkpoint 有两处展示标注应由 root 修正／明确。没有发现需要重跑构建、测量或重封 raw archive 的问题。**

2026-11-14，只读复核 `/workspace/zil-push-main` 的指定证据。本次没有构建、执行 compiler/native 或重测时序，没有修改 tracked 文件。读取时 HEAD 为 `8ce584f81a405b8c5aa69e555b80ede276f07143`；新增 checkpoint／custody 文件仍是 root 待提交证据，不将其冒认为已经远端发布。

## 1. 需要 root 修正的标注

1. checkpoint JSON 字段 `Rust_leading_median_and_paired_GM_cells` 列出的 18 格，实际是 **candidate 的中位数和 paired GM 均短于 Rust**。字段名方向反了。建议改成 `candidate_leading_over_Rust_median_and_paired_GM_cells` 或同样无歧义名称。数组内容与数值正确；Markdown 的中文方向正确。
2. `negative_observations` 只列相对 Rust 的负观察，但同时展示 main 比较且 Markdown 称“全部逐block样本及负结果”。原 rows 另有 **check/broad/block1 候选慢于 main**：candidate `663.060208ms`，main `620.657598ms`，约 `1.0683188×`。建议补列该实际负观察，或明确字段及文字只枚举“相对 Rust 的负观察”。这条样本已完整保留，没有删除或改值；不影响该 cell 中位数／paired GM 的结论。

O2 recovery 的相对 Rust 负值已经正确保留：block1 candidate `541.342419ms`、Rust `501.995707ms`，约 `1.0783806×`；两个候选样本 `[289.810002, 541.342419]`、Rust `[390.457242, 501.995707]` 未被筛掉，中位数 `415.5762105/446.2264745ms`、paired GM `0.8946557795419084` 均重算一致。cause 标为 Unattributed，正确。

以上为 outer 展示字段／范围说明问题，已即时回报 root。原测量、分析器输出及 sealed archive 无需改写；本报告没有代改 checkpoint。

## 2. 精确身份与内容绑定

| 项目 | 实读值 |
| --- | --- |
| compiled source／measurement tool source | `8ce584f81a405b8c5aa69e555b80ede276f07143` |
| candidate tree | `35a051da13de600c71ddc9ecb76edbe82cf23c85` |
| 当前 compiler SHA256 | `584e5ec59ec89fa83e69f4afb9a0f2185523ea8d0ebc1b671204168ad03ea810` |
| baseline source | `9170bb4210554392e36eb7528596612596ebef80` |
| baseline compiler SHA256 | `8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e` |
| strict analysis SHA256 | `55834c439b688a0b6ba9193a1c7cc28096f4bb1e85946013b53b9bb8a59dc38d` |
| pins SHA256 | `34072bc1fa156c29e8ec5055b70f3ef24baa8943cf69c332edab5a544da753c9` |
| 分析器脚本 SHA256 | `2e6582fee4f3ad6962dcbb83166ac3460b68d0c097d4a1d6f8ce3d66bc8d2a2e` |
| 本次 outer JSON 读取快照 SHA256 | `d218cff936beb34539ecdc5a18ff8c77770632da5d045525d9008436e4b6f290` |

已直接哈希当前候选／baseline compiler，均匹配 pins。两窗口 manifest 中 candidate source/tree、tool_source、binary before/after 与 pins 一致；归档 build receipt 的 bytes/hash、source 和 binary hash 亦一致。strict analysis 内嵌 pins 等于独立 pins 文件；analysis hash 和 checkpoint 的分析绑定相符。

两窗口 runtime.json 与 row runtime 相等：baseline 实际 DSO 为 `2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250`，candidate SDK18 DSO 为 `785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2`。两窗口归档测量脚本 hash 与 manifest implementation/implementation_after 相符。没有将旧 b6、021997 或 98fa 的窗口／二进制重标给本次 source。

本报告绑定上述读取快照；root 修正 outer 展示后应记录新文件 hash。compiled source、raw windows 和分析输出身份不因此改变。

## 3. 当前实际执行与明确继承收据

已逐一核 checkpoint 所引 receipt SHA、完整 source、exit/valid、实际 log SHA 和计数：

| 门 | 本 SHA 实际结果／命令范围 |
| --- | --- |
| backend | 273/0/0；`--lib -- --include-ignored` |
| driver | 211/0/0；`--lib` |
| Source | scalar_local_promotion 9/0/0 |
| retained | 本 SHA 扩充 retained_native_publication，`--ignored`，1/0/0 |
| CLI strict | all-targets，`-D warnings`，exit0 |
| changed backend/driver strict | 两包 all-targets，`-D warnings`，exit0 |
| fmt | 全树 check，exit0 |
| release | native-products CLI release，exit0，binary hash 如上 |

当前独审归档与 `/workspace/zil-recovery/final-reuse-chain-13-independent-review.md` 的 hash 均为 `42c1eab15962576a6c616ad302263d11b33c24e8e1c48059e6955e12a4b4edd8`；review-binding 的 source/base/patch hash 对应原 ACCEPT，未把当时尚未完成的测试状态改写成审查者亲跑。

native86 继承自 `b6e019894986b43a1bf1a81cf549a0850d66b9b4`；Expand116 继承自 `37fb410dd00e1f92b6ec660f313f8cf57d84fd71`。已核原 receipt hash/source/count，并执行记录中的限定路径 git diff，均确为 0 字节。checkpoint 清楚说明未在当前 source 重跑，当前 backend273／driver211 是另列的新执行，未混算。

## 4. 样本、质量与计算复核

两窗口均为原 original425、九请求、三臂、**两个 blocks**。已核 raw manifest/completion/rows 的 SHA 与 analysis 相符、内嵌 manifest 相等、controller exit0；没有把父请求计入 measured cell。

对每个 measured request／arm，从原 rows 按 block 选择，均恰为 `[0,1]`；18 格的所有原 wall samples 与 analysis／checkpoint 相等。独立重算中位数和同 block paired GM，均相符；Markdown 数字与这些值的展示精度一致。18/18 格中 candidate median < Rust median 且 candidate/Rust paired GM < 1 的有限窗口表述成立，不等于每个 block 都更快。

调用已审查分析器的纯结构 helper 重新核对原 metadata：完整 rows key、实际顺序、parent、pair 精确路径和对应三臂、native input／repeat 逐 row 均通过；与 analysis 的 pair_paths、pair_records、per_block 完全相符。还逐 row 对照实际 request.json、result.json，以及 ZIL response.json 和 runtime.json；fixture source identity 相符。

| 每窗口质量 | check | O2 |
| --- | ---: | ---: |
| rows | 102 | 102 |
| measured／preparations | 54／48 | 54／48 |
| pair receipts | 34 | 34 |
| native outcomes | 0 | 1080 |
| 完整 ELF／aggregate exports equal pairs | 0／0 | 30／30 |
| 每 block rows/measured/prep/pairs/native/ELF | 51/27/24/17/0/0 | 51/27/24/17/540/15 |

每窗口两条 ZIL 臂合计有 60 个成功请求报告当前 Verify=1、diagnostics=[]，8 个 error 请求在 Types SafeReject、Verify=0；与原 parent/error 布局一致。没有把 retained producer 的 native verifier 收据误标为当前 LLVM verifier 重跑。

strict analysis 标记 valid=true、结构校验 true、`artifact_rehash_executed=true`；O2 实际记录 178 个 unique blobs rehashed，check 为 0（没有原生 artifact），其数值与 scope 相容。本审查读回并绑定 root 的这次 fullhash 输出，没有重复执行全 blob 分析或 native 程序。时间与质量来自各自原始字段，不把是否命中当速度或质量证明。

## 5. custody 已完成，依赖和完成边界明确

已直接验证 9 个分片的每片 bytes／SHA256，并顺序拼接哈希：

- compressed bytes：`217597018`；joined SHA256：`3e00d02563d2ffa34dc20defd8f9391c21d8bb31bd8935f1595cb12002f6c725`。
- manifest inventory：10,359 regular files，合计 `1356679068` logical bytes。
- evidence-manifest SHA256：`7244364578767d9811b00dbef67b653bc6594850db1ca15c2f5762b3a5280501`。
- fresh-restore-receipt SHA256：`3c47dc90f4e133393acebd1aab6504c80c24c7b88a9c4425b63ca5c7ae552d63`，status=verified；source/tool=8ce584。
- verifier 实际文件存在，SHA256=`4a65fe492d5d5454e62d3965581a53e02e7ab6ca61214181919be996d256a18a`，与 RESTORE／custody 一致。

fresh root `/workspace/zil-final-reuse-13-evidence-verified` 存在；本审查另外实读恢复后的 compiler、strict analysis、pins，hash 与原文件及 inventory 一致。完整 bytes／size／mode／membership 是 root fresh verifier 的收据范围，本审查没有重新解包或声称另跑全部恢复校验。

SDK18/native-link 继续引用固定 V32 `1eecf9b81a4929dde79e4fa98cd0ee40fb74af75` 的 RESTORE，baseline 工具链亦是单独依赖。RESTORE/README 明确本 archive 不是整套 LLVM/toolchain 安装、原绝对路径需要显式重建／绑定，没有冒称重建 LLVM。独审与归档验证 metadata 作为 archive 旁文件加入后继 commit，sealed payload 不重封的安排成立。

## 6. 可发布结论边界

数值支持“本次两个 block 的 18 格 candidate 中位数及 paired GM 均短于 Rust”，并支持当前 final reuse 完整合同在列明新门／继承门范围内通过。文档保留启动范围、LLVM/CGU、OS cache、RSS 差异，没有内存优势推断、普遍稳定领先、main merge 或默认开关启用，也没有跨 b6 窗口作因果归因。

root 应在提交前修正第 1 节字段方向，并补列或明确限定负观察范围。除这两处展示问题外，未发现身份、样本、质量或完成状态错标。新 evidence commit 的远端推送及完整 readback 尚属 root 后续动作，本报告没有提前认证远端发布。
