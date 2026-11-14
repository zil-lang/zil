# 大项目 native 冷窗失败：完整 ELF 差异独立诊断

**结论：确认是实际机器代码变化，不能归为 metadata-only。当前全 ELF 门正确拒绝；保留该窗失败，不能据此发布性能/磁盘优势。此次未证明存在语义错误，也没有证明两份代码在完整合同上等价。**

窗口：`/workspace/zil-push-main/out/runs/large-project-large1000-edits-01-o2`，block-00 / 000-cold。只读原 ELF、对象、LLVM 源、逐臂收据及 root 已准备的反汇编。没有改原证据、compiler、runner、门或 pin，没有执行 Git、构建、native 或 benchmark。唯一写入是本报告。

此前工具两项 P1 的冻结复核已经交付：[large-project-tool-review.md](/workspace/zil-recovery/large-project-tool-review.md)，绑定 tool `ad81486b976634e71bf1c61544d0fd28a783fbae` / base `6958d700e4d09b050d70d930e7f296c41e1275ae`；不因本次实际 run failure 改写那份源码复核结论。

## 原失败身份与已过边界

本窗参数：1000 preset、100 modules、short、wide、32 private-edit epochs、O2、push candidate、2 blocks、三臂 CGU16/jobs1。实际停在首个 cold triplet；后续 edits/error/recovery/restore/restart 均未执行。

| 身份 | SHA / 数量 |
| --- | --- |
| baseline source | `9170bb4210554392e36eb7528596612596ebef80` |
| candidate source | `8ce584f81a405b8c5aa69e555b80ede276f07143` |
| tool source | `ad81486b976634e71bf1c61544d0fd28a783fbae` |
| baseline ELF | `8b3ba23551d3a54ab07151897eb521298e2ab2a9458169d5801a30ffa851f647` |
| candidate ELF | `c5b994710fee0561196ab9bf497bdbb2c7ebd687dfc3012d579b643c91eca316` |
| 两份 ELF 大小 | 各 202752 bytes |
| 实际 / 预期 rows | 3 / 246，尚有 243 行未执行 |

独立重哈希 ELF，与收据 SHA 相符。已读三臂 `large-request.json`：各自 valid=true，各有 12 条 native 结果且全部 valid。两条 ZIL 臂 verification_runs=1，executed Verify event 各恰一次。原 `pair-validation.json` 为 semantics_equal=true、native_oracles_valid=true、physical_zil_units_valid=true、per-unit/aggregate exports equal=true，但 executable_equal=false，valid=false。Rust 对象 inventory 本窗实际也为 16。

`semantics_equal` 在该 runner 中指具名报告字段的一致，不能被解释为任意输入上的机器语义等价证明。12 native 次数对应六个固定输入各两轮，也不是完整输入域证明。

原 [summary.json](/workspace/zil-push-main/out/runs/large-project-large1000-edits-01-o2/summary.json) 已保留 valid=false、comparisons=[]。其中 failure.arm=rust 是三臂循环最后处理的 arm 上下文，不表示 Rust native 执行失败；实际失败发生在之后的跨臂 full-ELF 质量比较。

## section 与全文件定位

直接解析两份 ELF64 section/symbol tables，并逐字节比较完整文件：

- 总共 **1584 个字节不同，全部位于 `.text`**；文件 offset 的首/末差异分别为 `0x7ae3`、`0x25d21`。
- `.text` 两侧均为 155438 bytes，file offset / virtual address 均为 `0x1040`；长度和整体布局相同。
- `.text` baseline SHA256：`d78d2f4704ebbd9f0e046212ba10d605f2b414d180bb1b54513c25af30b60126`。
- `.text` candidate SHA256：`84fb51cc55eddabaf79fe57d2481a3934af0e35103ef0d0508a7dd4eb513c250`。
- `.text` 以外的全部文件字节相同：包括 ELF/program/section headers、padding、symbol/string tables、relocations、dynamic/GOT、notes、unwind、rodata/data 等。没有 metadata 差异需要解释，也不是 build-id/文件名/时间戳造成的 ELF 不等。

两侧符号表完全相同，函数地址、长度没有漂移。按非零大小 STT_FUNC 符号解析，共 1005 个；只有以下 8 个函数 body 不同，其余 997 个相同。变化字节精确被这 8 个符号范围覆盖，不存在符号外的额外差异。

## 全部受影响函数

通过 main 的 100 个 `mN.value(x)` 调用顺序与归档 LLVM main 中的实际 callee 对齐，核得如下源码归属；不是仅凭符号数字猜测。源 main 的完整调用顺序为 m0…m99。

| 符号 | 源定义 | CGU index（0 起） | ELF 起址 | body bytes | 差异 bytes | root 反汇编起行（两侧相同） |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| zil.fn.333 | m32.value | 2 | `0x7ad0` | 853 | 198 | 8243 |
| zil.fn.983 | m97.value | 3 | `0xbb60` | 853 | 198 | 13323 |
| zil.fn.203 | m19.value | 5 | `0xf050` | 853 | 198 | 17490 |
| zil.fn.853 | m84.value | 6 | `0x130c0` | 853 | 198 | 22563 |
| zil.fn.73 | m6.value | 8 | `0x159d0` | 853 | 198 | 25798 |
| zil.fn.723 | m71.value | 9 | `0x19450` | 853 | 198 | 30405 |
| zil.fn.593 | m58.value | 12 | `0x1f760` | 853 | 198 | 38220 |
| zil.fn.463 | m45.value | 15 | `0x25ab0` | 853 | 198 | 46045 |

八个 module index 都满足 `i % 13 == 6`，对应生成器系数的重复形状。**每一臂内部，这 8 个函数的 853 字节 body 逐字节相同**，所以可以只深入分析一对代表 `zil.fn.333`，再以 byte equality 覆盖其余七对。

| 同一臂内八份 body 的共同 SHA256 | 值 |
| --- | --- |
| baseline | `f456b4c88fab545059b6a6f2f5d3e7a948c4738be558c98b97d37ef91592e588` |
| candidate | `9901bfb8c4f2d0522576b71bb6847ce9939f0195e27b4da581efae50acbf98ff` |

按符号大小边界数指令，每份是 **236 条**，两臂 mnemonic histogram 相同。其中包括 35 imul、58 add、31 mov、3 lea、13 jo、1 js、1 jbe、1 jmp、1 ret、1 ud2。root objdump 文本在该函数标签下还显示末尾对齐 `nopl`，但它在 853 字节符号范围外，不能误算为第 237 条函数指令。

## 代表性指令差异及分类

完整原始差异见 [disassembly-diff.txt](/workspace/zil-push-main/out/runs/large-project-large1000-edits-01-o2/disassembly-diff.txt)。下列地址属于 `zil.fn.333`，其余七对具有完全相同的相对变化。

**1. 同一值的寄存器分配变化。**

在 `0x7b18`，baseline 为 `mov %edi,%r10d; add $0x5,%r10d`，candidate 为 `mov %edi,%r9d; add $0x5,%r9d`。之后消费该值的 sign-extension/模运算链相应调整；另一组值从 baseline 的 r9 换到 candidate 的 r10。后缀中多处 `movslq/imul/shr/add/sar/sub` 的寄存器操作数随之变化。

这部分形状符合寄存器/临时值分配变化，但完整差异不能简化为一次全函数统一寄存器重命名，因为下一项同时改变了汇合前的 copy 放置与指令序列。

**2. LEA/ADD/MOV 的选址、顺序和汇合 copy 变化。**

baseline 的非整除分支末尾为：

```text
0x7b48  lea 0x3(%rdi),%r11d
0x7b4c  lea 0x9(%rdi),%r9d
0x7b50  add $0x6,%edi
0x7b53  jmp 0x7be4
```

candidate 对应序列为：

```text
0x7b48  lea 0x6(%rdi),%ecx
0x7b4b  lea 0x3(%rdi),%r11d
0x7b4f  add $0x9,%edi
0x7b52  mov %edi,%r10d
0x7b55  jmp 0x7be4
```

在这个分支中，原 x+6 从 baseline 的 edi 改为 candidate 的 ecx，原 x+9 从 baseline 的 r9d 改为 candidate 的 r10d。另一条分支末尾，baseline 的 `0x7be2 mov %ecx,%edi` 在 candidate 中没有对应 copy；汇合点 `0x7be4` 则从 baseline `movslq %edi,%rcx` 改为 candidate `movslq %ecx,%rcx`。

这是实际数据流/phi-copy 实现选择及 instruction placement 变化。两侧总长度和静态指令数相等，不代表每条动态路径的指令数、依赖链或执行成本相同；不能据总量相同宣告代码质量相同。

**3. 条件分支落点和块内地址改变。**

`0x7ae2` 的 jbe 从 baseline `0x7b58`（函数+0x88）改为 candidate `0x7b5a`（+0x8a），相应分支前部指令地址顺延。共同 join 仍是 +0x114，trap 仍是 +0x353、ret 仍是 +0x352。检查到两侧均保留 13 条 jo、1 条 js、共同 ud2，没有观察到直接删除 trap 路径或新增外部调用。

落点偏移是代码布局改变的结果，不是要从完整 ELF 比较中剥离的“元数据”。分支/flags/overflow/trap 是否在完整输入域上保持相同，还需要语义层复核。

## 差异在链接前已经存在

逐一读取 16-unit inventory 指向的原始 `.o` payload，独立重哈希后，按 ELF symbol table 提取上述八个 body：

- 每个 `.o` 中的相应 853 字节 body 都与同臂最终 ELF 的该函数 body 完全相同。
- 每个相应函数范围内 relocation 数均为 0。
- baseline `.o` 中已是共同 `f456b4…` body，candidate `.o` 中已是共同 `9901bf…` body。

因此不能把这次差异归因于 native-link manifest、链接器填充/重定位、最终 ELF 元信息或归档操作；差异最迟在生成对象时已经出现。

现有收据也不能支持“MIR 重放单独导致全部差异”的归因：第一个受影响 CGU 2 的 candidate 统计为 machine_function_reuses=0、cohort.machine_replays=0，而它已经包含不同的 `zil.fn.333`。该 CGU 同时有 16 次 IR stage reuse；后续受影响 CGU 则有非零 MIR reuse。因此需要区分原始 IR 输入表示、O2 优化选择、IR stage reuse 和后续机器复用，不能直接认定某个机制有错。

## 与归档 IR 的联系和归因限度

两臂归档 LLVM 文件不是同一个输入表示：baseline 为 2351528 bytes，candidate 为 1409218 bytes，SHA 分别为 `d85f089cf3f5ae329aeddaaa4609e78ddabf5c38dac7dbf7c56953ae83dd43dd`、`7e4ebbe4e8fbc720d5da56e3081513ca1215d0e3d631ec02721949b6af70604e`。

上述八个 `value` 定义本身，在这两份归档 LLVM 中各自对应的原始定义文本完全相等。代表 `zil.fn.333` 分别位于 baseline 第24626行、candidate 第14561行。但它调用的 helper 输入表示不同。例如 `zil.fn.325`：

- baseline 含本地 alloca、store/load、常量构造和余数防护分支。
- candidate 直接使用 arg、常量余数，显式保留 checked multiply/add，并用 `%typed.phi.3.1` 汇合两个结果。

这与候选的早期标量 SSA/常量正规化表示相符，说明不能只比较相等的外层 value 原始定义便认定其完整 inline/优化依赖相同。本次归档与反汇编能定位差异形状和产生阶段上界，尚不足以把首次分歧精确归到某个 O2 pass、IR replay 或 codegen heuristic。

## 后续语义复核的最小交接范围

当前最准确的分类是：**一对重复的 853 字节机器码，在八个聚合函数上表现为寄存器分配、分支汇合 copy、指令选址/顺序和局部块布局变化；不是元数据差异，也没有已确认 wrong-code 反例。**

若 root 要进一步判断是否属于可接受的等价 codegen 变化，可把 `zil.fn.333` 两份 body、其两个分支、join 后的完整 signed modulo/aggregate 链及 helper325…332 的对应输入交给 Astra 独立语义复核；其余七份可由已核 byte equality 覆盖。复核应包括：

1. 分支到 join 的寄存器角色映射，以及 x+6/x+9 copy 改放后的全部用途。
2. signed remainder、flags、checked overflow 到 ud2 的条件和顺序；程序可达 x∈[0,250] 与函数一般 i32 合同须明确区分。
3. ABI/callee-saved rbp/rbx、返回值及无额外内存/调用副作用。
4. 代码质量是否仍符合事先声明的验收合同；静态指令数相同和六输入通过不替代这一步。

本报告只提供具体差异归类和最小证据范围，没有执行该完整语义证明或申请放宽门。若将来提出新质量合同，也必须另行审查并明确与本次首次失败的关系；不能追溯把该窗改为 Pass。

## 关键证据路径与摘要

所有路径均在原窗口下，未作修改。

| 证据 | SHA256 |
| --- | --- |
| requests/block-00/000-cold/pair-validation.json | `e61d9fa2ac8887b9c4dc6575fafd02309a69af4d39bb063cf8667614413ddd37` |
| baseline-disassembly.txt | `338e5c01bda4fcbeed6e8a6b575aad5d80dfcc7d1064957ea57b512412bcd914` |
| candidate-disassembly.txt | `abaecce8d18154f3639c8efa4332709fed63001514ddbc4e91ce450dc299c3fb` |
| disassembly-diff.txt | `7f0e21d441e4670057c39a2852955a753fec89651dd707468c963b7e12775d18` |

代表性对象：baseline CGU2 `blobs/cccd41c3b96d624c89bbc2f587b9a2ecc59ef9936607b86024b6109175af5279.o`；candidate CGU2 `blobs/db3fe21ccd29f18b07a0bd0fd50ed8c255f16e20b1a88bd5e8838537a08e4d4c.o`。其文件 SHA 与文件名、inventory 相符，前述 body 差异在这里已经成立。

**裁决保持：原窗口 valid=false、comparisons=[]。完成了具体 native 代码差异诊断，未取消或放宽 full-ELF 门。**
