# 1679运行回退的静态机器码归因

结论：cost32 bounded imports在本语料中截断了根函数对跨文件大callee的优化可见性。产物直接显示更多真实函数调用、根函数packed SIMD消失，以及N256主循环重新保留溢出检查；这不是仅从wall变慢猜出的解释。完整运行门仍是8/8 NoGo，不能以Rust/B通过、减小代码或某项编译时间改善豁免。

这里只读取现有24个A/B/R ELF、原始LLVM sidecar和已有216行运行结果，运行nm/objdump进行静态解析；没有执行产物、编译、采样或修改生产代码。完整符号表、选定函数反汇编和全部8case静态计数在同目录，结构化结论见machine-code-attribution.json，详细原始解析见static-evidence.json。

## 改变发生在根CGU的优化可见性

全部8case的A/B `program.ll`原字节完全相同，因此输入的完整prepartition LLVM模型没有变化。它不是优化后的unit IR。最终ELF不同，8个unit中其余7个unit的object_bytes逐项相同，变化集中在根unit：

| base case | 根unit对象字节 A→B | 其余7unit字节 | 根value符号 |
| --- | ---: | --- | --- |
| N64 | 104152→35352 | 全相同 | zil.fn.2 |
| N256 | 209704→127072 | 全相同 | zil.fn.2 |

源码差异仅将既有resource-helper分区路径推广到无closure/helper的多文件程序；原先这些程序进入普通完整callee实现导入路径。新路径仍按cost32限制跨文件source body，所以根value只看见大callee的声明。后端、完整O2管线以及原始完整IR没有被改写。单凭对象尺寸不能证明对象字节相同；这里的优化损失由下列实际反汇编补足。

## 内联和跨调用SIMD机会消失

`zil.fn.2`对应根模块的value；`zil.fn.14`对应m1.value，后续每个文件的value编号增加9。表中为静态函数体计数，不是硬件动态指令数或周期：

| base case / arm | 根value字节 | call位点 | jo位点 | 使用向量寄存器的指令 |
| --- | ---: | ---: | ---: | ---: |
| N64 A | 39577 | 7 | 34 | 4799 |
| N64 B | 912 | 63 | 62 | 0 |
| N256 A | 84363 | 31 | 34 | 2437 |
| N256 B | 3792 | 255 | 254 | 0 |

所有4个N64 snapshot都从7个call增至63个；所有4个N256 snapshot都从31个增至255个。正常输入路径中根value没有包围这些call的业务条件分支，main每轮调用根value一次、共100000轮，因此分别增加560万和2240万次叶子value调用。这里不把Rust异常路径的静态call数计入正常调用数。

N64 B的根value开头实际是逐callee调用、累加、溢出检查：

```asm
1146: call 6530 <zil.fn.14>
114b: mov %eax,%r14d
1150: call b8a0 <zil.fn.23>
1155: mov %eax,%ebp
1157: add %r14d,%ebp
115a: jo 14ce <zil.fn.2+0x38e>
1162: call 10a40 <zil.fn.32>
1167: add %eax,%ebp
1169: jo 14ce <zil.fn.2+0x38e>
```

A的同一个根value已经含大段packed算术，例如：

```asm
162b: pmullw ...(%rip),%xmm4
163b: paddw  ...(%rip),%xmm4
164c: movdqa %xmm4,%xmm0
1650: pmulhw %xmm13,%xmm0
1655: paddw  %xmm4,%xmm0
1661: psrlw  $0xf,%xmm3
1666: psraw  $0x7,%xmm0
166b: paddw  %xmm3,%xmm0
```

N64 A根中有359条pmullw、594条paddw、181条pmulhw；B根中这些指令全部为0。这与内联后跨调用的SLP式打包机会消失一致，但没有保存LLVM优化remarks或pass dump，因此不指定究竟由哪个pass、哪一条pipeline子阶段完成，也不把CSE贡献单独量化。

这不等于关闭了整个程序的向量化。m1.value（zil.fn.14）在所有8case中A/B的助记符计数仍一致；base的两arm均为300条静态指令、85条使用vector寄存器、15条jo，本地body内仍有pmullw/paddw。损失在于跨叶子函数的可见性和组合，而非局部后端突然换了低优化等级。

## 范围与溢出检查也有具体退化

N256 A的main（zil.fn.5）在根value返回后的acc累加没有jo：

```asm
15b1d: call 1140 <zil.fn.2>
15b22: inc  %r14d
15b25: add  %ebx,%eax
15b27: movslq %eax,%rbx
```

B同位置保留检查并跳往ud2：

```asm
205d: call 1140 <zil.fn.2>
2062: add  %eax,%ebx
2064: jo   20c8 <zil.fn.5+0xb8>
2066: inc  %r14d
```

这在全部4个N256 snapshot成立：A main有1个jo、B有2个；A在该位置已经能够消掉检查，B没有保住这一优化。没有优化后的IR/分析remark，所以只能把“检查被重新保留”列为事实，把“丢失跨函数范围信息”列为与源码可见性变化一致的解释，不点名断言某个range分析pass。

N64的main反汇编在忽略地址/RIP重定位后形状相同，都是59条指令、2个jo；相同100000轮和取模仍存在。N64的回退不能归咎于main多跑了循环。N256的循环次数也相同，只多了上述检查等局部变化。

## Rust产物作为有限对照

已保留各case的R符号表和main/m1.value反汇编。N64 Rust m1.value在此产物中为521条静态指令、0条vector寄存器操作；ZIL A/B的同义叶子为300/85。Rust函数尾部还包含检查失败路径的call，它们不能算作正常执行调用。该差异不能独立拆解Rust/B的2.17–2.327倍，也不能外推成通用Rust运行性能结论；更不能用来救B/A全部失败。

## 是否有小而可信的消除重复优化路径

本证据没有建立这样的修复。此次缩短编译上下文省掉的恰好包含让根函数获得内联、SIMD组合和范围优化所需的body；把这些计算变成外部调用，不是同质量地消除重复优化。

仍可信的局部方向是保留完整imported IR，将非owner的body作为available_externally类定义给LLVM内联使用，避免未内联body在多个unit重复发射。它可能减少重复发射、命名和重写成本，但不会自动消除每个CGU对该body进行的重复优化，且当前产物没有验证这一方向的语义、优化决策或净收益，不能当作已经成立的小修复。

但这不是尚未探索的新候选。历史提交`8255079de3434de5aad44805a12eed8e9f433c24`的`changes/research-incremental-o2-quality-closure.md`已经比较boundedAE、completeAE、boundedInternal、completeInternal四臂：bounded AE保留原退化；complete AE虽恢复arithmetic，但诊断optimizer成本更高，且branch运行质量没有完全恢复；complete Internal才同时恢复该轮质量。实现提交`b9e566e02cf69b217a2d47c2377340771b02f161`采用private Internal clone，理由包含未发生内联的IPO结果也必须真正发射并可调用，AE body会被丢弃。上述是历史诊断证据，不是本轮新的正式性能样本；完整索引见[available-imports-history-audit.md](../../multi-edit/available-imports-history-audit.md)。因此前述AE方向只保留为设计边界说明，本轮不视为新候选，不全量重试旧四臂，也不能声称简单换linkage就能恢复质量并获益。

若目标是消除optimizer本身的重复工作，同时保住A的根机器码质量，需要完整优化unit/调用者特化结果的增量缓存、精确import body依赖以及summary/import规划，或ThinLTO式后端增量架构。内联消费了某个callee body的caller，在该body变化时必须失效；不能只凭接口或按loop有无掩盖这条依赖。现有完整对象缓存可以复用未变化CGU，新增机制必须在保留完整优化可见性的前提下继续减少工作。

本报告不调整cost32、不挑case、不按loop回退、不新增运行，也不改变1679的NoGo。
