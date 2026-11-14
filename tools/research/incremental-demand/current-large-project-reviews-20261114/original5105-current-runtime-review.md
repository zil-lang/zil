# 原5105输入：current capacity runtime 收据独立复核

**裁决：接受该精确候选在原输入上的限定 capacity 整链成功收据。未发现 candidate 身份串用、源裁减、零执行或最终伪 success。main9170 的四次 Expand ResourceLimit 保留为失败，不能计算与它的性能比或宣称完整 ELF 同等质量。**

窗口：[large-project-original5105-current-01-1de00f63f80c](/workspace/zil-push-main/out/runs/large-project-original5105-current-01-1de00f63f80c/summary.json)。只读该窗口、具名原源、release17 和相关小型原始记录；没有执行 compiler/native/benchmark、没有构建、没有重哈希 compiler/DSO/大 ELF Blob，也没有改 source 或原证据。唯一写入为本报告。

## release → candidate binding 身份链

| 项目 | 精确值 |
| --- | --- |
| release17 / runtime source | `1de00f63f80cf4534ec995dc51e106005b270ddb` |
| source tree | `850ff626b6e9cc227d29b2cbe823fa769df52c8c` |
| release 与 runtime binding 的 binary SHA256 | `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7` |
| release 生成路径 | `/workspace/zil-target-closed-body/release/zil` |
| 本窗实际 argv 的 candidate | `/workspace/zil-push-main/out/binaries/1de00f63f80cf4534ec995dc51e106005b270ddb/zil` |
| baseline source | `9170bb4210554392e36eb7528596612596ebef80` |
| baseline binary pin | `8d8f7ddbd9bcf9567e042a7a8256a58698897589cafc0961655ba0a24113514e` |

已读 [release.json](/workspace/zil-push-main/out/runs/large-source-native-release-17-1de00f63f80c/release.json)：command 是 locked/offline/release 的 zil-cli 构建，features=native-products，exit/exit_code 均0、valid=true；没有把 test_results=[] 当测试通过。读取提交对象核得完整 source/tree 与 receipt 一致。另重哈希其1107字节 release.log，与记录的 `0dffc41516501cef922e2f92ed899c542ed46ac0c5f4d56acbf1266725e41a86` 一致；completion 的 source 也一致。

[binding.json](/workspace/zil-push-main/out/runs/large-project-original5105-current-01-1de00f63f80c/binding.json) 的 source、binary hash 与 release 完全一致，四个 candidate 请求的 resident command 实际使用上述不可变 binary 路径。不存在用另一份 stock 30c/aaa 或旧研究 binary 的 SHA 填入此次结果的收据不一致。

这里核的是已存在 build/harness 收据之间的绑定，以及实际 argv；按用户要求没有再读取完整 compiler/DSO 内容重算 hash。harness 曾在准入时计算 candidate binary hash，不能把本次报告说成又独立重哈希了大二进制。

运行使用 `/workspace/zil-measure-ad814` 的 helper 快照；三份归档 helper 与 binding 及该 checkout 的字节 hash 一致。native profile ID 为 `192bd5739d2420fb3f15afe5c1672ec4934c94046259a79d642e9ffc828f65e6`；candidate command 的库目录为 LLVM function-products-v18。此处不新增实际 DSO mapping 或 SDK 全闭包验收声明。

## 完整原 Source、file hashes 和当前 Map

独立只读核算了原目录与本窗口 `sources/` 两套102文件的全部 SHA256，二者都与具名 fixture 的完整 file_hashes 字典相等：

- 文件集合：main.zil、io.zil、m0.zil…m99.zil，没有少模块或裁掉定义。
- 总源字节：**843875**。
- 原身份：`892fbc295a4cd51abec61c1cd600079126b1d473e1a85671f506353dfe333627`。
- 具名 fn inventory：**5105 项，含2个 extern 声明；5103个有正文函数**。这与 cold Verify 的5103正文证明/5105 raw inventory 相符。
- 原 fixture.json 的当前 hash 仍为 `1c0d646573d16a65c96e80da7af0ce0ee6a49ca0e2b047229695e98155c07e03`，等于本窗记录的 original_fixture_sha256。

八个当前 ZIL response（candidate与baseline各check/O2×cold/warm）都实际含102项 source_files，路径集合精确绑定本窗这102份源。candidate 四次 response 的 attempt 内 Map 与外层 Map 完全相等，均只有一个 attempt、retry_reason=null；没有以重试到删减输入的结果代替原输入。

candidate 四请求当前 input_fingerprint 均为：

`4f66716f34e7fb79e32de4c5a3551a0f045655795c564a128af54e3828e539d0`

源内容 identity 与程序 input_fingerprint 是不同字段，未将它们混称。后者在当前四次请求及各自 attempt 中一致。

## 最终汇总和实际执行矩阵

顶层 summary 为 complete=true、candidate_full_source_verify_native_success=true、rust_check_receipt_and_o2_success=true、protocol_valid=true、diagnostic_success=true、formal_ratio=null。11条 rows.jsonl 与顶层内嵌 rows 完全相等；candidate/Rust 当前叶 summary 也与相应已 emit 行一致，没有采用之前 P2 的 provisional 真值。

| 臂 / 请求 | 实际结果 | 当前 ZIL Verify | native oracle 执行 |
| --- | --- | ---: | ---: |
| candidate check cold | Success，phase=verify | 1 | 不适用 |
| candidate check warm | Success，phase=verify | 1 | 不适用 |
| candidate O2 cold | Success，phase=link | 1 | 12/12 |
| candidate O2 warm | Success，phase=link | 1 | 12/12 |
| main check cold / warm | 两次 ResourceLimit，phase=expand | 各0 | 无执行 |
| main O2 cold / warm | 两次 ResourceLimit，phase=expand | 各0 | 无执行 |
| Rust check historical-cold | 成功历史收据复用 | 不适用 | 不适用 |
| Rust O2 cold | 本轮 Cargo成功，artifact fresh=false | 不适用 | 12/12 |
| Rust O2 warm | 本轮 Cargo成功，artifact fresh=true | 不适用 | 12/12 |

四个 ZIL resident 均有 ready/protocol=1，原 stdout.jsonl 恰为 ready+两次响应，分别与保存的 ready/response.json 相等。每个 resident 请求号为1/2、session_epoch=0、session_cleared=false；cold/warm 不是两个不同 resident 伪装成暖缓存。四个 resident 的 exit.json 均 returncode=0、forced_shutdown=false。

root 所报 harness exit0 与 complete/diagnostic_success 及已审 harness 的最终 return0 分支一致；controller log 完整记录了11行。没有把 resident正常退出0代替编译结果：main 的 resident也正常退出，但其四个编译响应仍严格算失败。

## 当前 Verify 与实际 O2 工作的区别

candidate 两个 cold 请求均有102次 parse executed，并各执行 expand/CT/Types/Analyze/Verify；O2 cold另有lower/link executed。Types body_checks=5105；cold Verify 中 function_proofs_checked=5103、raw_shape_functions_checked=5105，full_receipts_reused=0。

两个 warm 请求复用 parse/expand/CT/Types/Analyze 结果，但 **Verify event 仍恰有一次 executed，verification_runs=1**。其 verification_work.full_receipts_reused=1、function_proofs_checked=0；因此可以称“当前独立 Verify 调用及绑定成功”，不能称“暖请求重新逐正文计算5103份证明”。

O2 当前报告两次都观察到 backend=llvm-api、requested_units=16、actual_units=16、jobs=1、fallback=null。进一步核对：

- cold 有16条实际 object_reports，全部 pipeline=`default<O2>`、verify_before_after=true；没有用 argv 的16假装16条报告。
- warm 的 final_artifact.reused_program=true、retained_files=true、before_codegen=true，current_preflight_checked_units=0、current_preflight_reused_units=16、llvm_verification_scope=retained-producer、current_llvm_verifier_executed=false，当前 object_reports 数为0。
- warm 同时有当前 lower/link query 执行事件；这不表示实际重新执行16次LLVM codegen或重新链接。它表示当前请求经过对应消费者并接受先前producer的已绑定产物。

上述域均在原始 response 中明确区分，未把 retained-producer 冒报为本次新的 LLVM verifier/codegen。这一 capacity 接受不升级成完整 physical-CGU 对象归档独审。

candidate cold/warm 记录的程序 archive 均为1058960 bytes，收据 SHA 同为 `4f3c39d01498f67053f4af0c89da098958a1eba47c178971230c6a1f5a8d1787`。本审查仅核 ELF 头、stat 和收据字段，不重哈希或逐字节比较这两个大 ELF。

## 六输入×两轮：独立下钻48份原始运行记录

不是只读 all-valid 布尔值。本审查逐项读取 candidate/Rust × cold/warm 的 **48个不同 native目录**，对每条核：oracle.json与行内观察相等、result.json相等、实际命令只执行当前 row指定program、cwd与对应叶目录一致、stdin.bin字节正确、returncode=0、timed_out=false、stdout/stderr原始字节及其记录hash相符，wall_ms为有限正值。

每组明确是6输入×2轮，共12次，没有零执行或重复引用同一目录凑数。独立用100×50×1的纯整数定义重新计算六个 oracle，不执行编译器/可执行文件；与每条 expected及实际原始输出均相等：

| stdin hex | 完整 stdout hex | exit / stderr |
| --- | --- | --- |
| 空输入（EOF） | `d0880000` | 0 / 空 |
| `00` | `d0880000` | 0 / 空 |
| `31` | `64560000` | 0 / 空 |
| `41` | `33950000` | 0 / 空 |
| `80` | `927c0000` | 0 / 空 |
| `ff` | `acd50000` | 0 / 空 |

这些都是四个原始输出字节，不是截断为低8位的返回码或格式化文本比较。四组各12次均符合该表。它仍只覆盖具名六输入，不是任意输入域或全部语言能力证明。

Rust当前102份源的完整metadata与三个Rust行的source字典相等，889538 bytes，identity为 `747f7019541040f316f10866dbc1bab8c0375379b0751209152bb4dba8d5ea19`。历史check复用的summary与原路径原字节相同、receipt_sha256正确、成功result及stdout/stderr hash均符合，唯一build-finished=true；没有混称为本轮重新计时/重跑check。

Rust O2 cold/warm 的原stdout Cargo消息与归档 cargo-messages相等，均唯一成功build-finished、退出0、未超时，唯一程序artifact指向该窗口release目标。cold为fresh=false、warm为fresh=true，恰与新target首次构建/后续Cargo复用一致；native执行在两种情况下都实际进行。

## 默认预算、main失败与结论限制

逐token核对了四个ZIL command与预期命令：完整原main.zil、resident、semantic cache；O2为llvm-api/O2/CGU16/jobs1/object cache；只有candidate有push-reuse及O2 native-link profile。没有 max-*、stop-after、跳Verify或compiler timeout/budget override。Rust manifest为opt-level=2、codegen-units=16、incremental=true、lto=false，实际Cargo命令为offline/locked/jobs1/release及本窗target。

这是默认编译器预算下的容量结果；外部watchdog、系统资源、research/SDK身份和resident生命周期仍属于该具名诊断条件，不推广为任何环境、stock版本或普遍性能结论。

main四次原始响应均为 ResourceLimit/expand/V0，诊断为 `E-RESOURCE-LIMIT: generic expansion budget exceeded`；cold实际parse102文件，warm复用parse后再次尝试expand。对应目录没有发布program，原错误/Map完整保留。不能把 main失败当作 candidate更快，也不能拿其失败产生的空产物/小缓存作磁盘优势。

本窗口没有成功的 main native ELF，因此 **不存在 main/candidate完整 ELF相等的质量对照资格**。本次接受的是“candidate成功处理具名完整源且真实原生输出符合独立oracle”；不是追溯放宽先前 large1000的full-ELF失败门，也不是正式matched ratio、冷暖速度领先、stock合入或语言父门通过。

## 精简证据索引

| 小型收据 | SHA256 |
| --- | --- |
| release17/release.json | `bb2fe7a9c3c76c1f5780f31821ff54c61717ce08f5890735ac9c300deedd07a7` |
| 本窗 binding.json | `85c8efa5026841fc16a4c40cd2d71e7cbc7b7f4ae3942c25915c6ca57d94c76a` |
| 本窗 fixture.json | `8c0ffeca7d66c5ccda089b0af3861f6a78cd59ea7e4c220f97c2855f1959bbbf` |
| 本窗 summary.json | `6476ae683c57739ae888376e38fd335e1de14fc60abadc949ed3e17148baa7d0` |
| 本窗 rows.jsonl | `ce9c7ccc019a6c46ead185f65b7822bc6a85dc9285fcf65677dc7787b310d012` |
| candidate/check/cold/response.json | `22736453b180fec4d753cc8fd8a3443570341e09be075bee786a9aea5f07a0bd` |
| candidate/check/warm/response.json | `eb2eae7b89c1d76f0f078ca645dd6d4b4565921a8e22ed09ae67f66e87ec57da` |
| candidate/o2/cold/response.json | `ea028dd762ecfdc6f9d32127e0ba0c56d6141d11c7aadf4f4cfde650d8607d8b` |
| candidate/o2/warm/response.json | `b735b38318cb3d3bfd9f26ffb554cb346717ed5fa2579810c442f5bc00d8c51e` |

**最终：该原5105/default-budget容量整链收据接受；身份、完整源/Map、当前Verify及48次非零native原始观察一致，无确认伪成功。main失败和所有非formal、暖复用、未重哈希大Blob的边界原样保留。**
