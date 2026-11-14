# post-O2 emission：精确源码独审

**裁决：c4 的新链路具备可继续验收的有限实现；本次未发现可由已审核 structured 输入直接触发的 wrong-code、跳过当前 Verify/preflight、跨请求复用授权或 partial publication 漏洞。但 c4 整体验收仍为 Hold：首 gate 13过/1败，实际大输入覆盖、其余统一门尚未完成。不能把这一结论写成全部源码/质量门已接受。**

对象：`c4f869af46efdb3aef5203bf8b69c5d1d084b614`，tree `955b4523e8ec0f985d2349e8f154a7dded7708a5`。主报告绑定该提交，不将 root 后续 ConstantRange 修改及 owner 的测试修正归入 c4；交付前新提交已出现，另列文末增量。开始时 checkout=该SHA且干净；审查期间观察到 emission_assembly_tests.rs、随后 emission.rs 的未提交修改，涉及冻结文本的后续核对使用 `git show c4f869af:<path>`，不将新内容混入 c4。

审查范围：API `lib.rs`、`native.rs`、`native/emission.rs`、`native/emission_assembly.rs`、其测试，backend `build.rs` 与既有 object/final-product/cache/publication 消费者，driver `session.rs` 的选择和当前验证/发布路径。同提交 packed_items/semantic_items 的独立持久化改造不在本报告验收范围。已按仓库 AGENTS 和本会话只读限制执行；未运行 compiler/build/test/benchmark，未写生产或执行 Git mutations，仅写本报告并读取 root 原始运行收据。

## 1. 发现与未闭合项

| 项目 | 分类 / 裁决 |
| --- | --- |
| token测试 `%eax` 数量期望4、实际6 | **已证实测试缺陷；c4首gate失败保留。** 不是已证实ASM替换错误。修正后另绑定新SHA/收据。 |
| ConstantRange 属性导致代表性 rem251 叶被拒绝 | **保守 miss，非wrong-code。** root报告了真实形状；c4源码确实拒绝所有非enum属性，但本审未运行该大输入，不能为c4宣称真实大项目收益。 |
| callsite属性组未单独typed绑定 | **证明边界待补，不升级为已证实漏洞。** c4只绑定function/return/param属性；见下文对受支持生产域的限制及建议。 |
| 新对象与ordinary disk-cache namespace未区分emission策略 | **控制实验/诊断身份风险。** 同binary同cache切换可能先命中对象而不执行指定producer；验收必须分cache或直接API fresh对照。既有current preflight仍执行，不是旧对象绕过验证。 |
| 相同优化IR文本不保证任意情况下machine bytes相同 | **完整ELF质量风险仍需实证。** use-order/numbering等未全部绑定；未发现由此破坏已限定封闭叶语义的具体反例。不可从12函数用例外推全部输入。 |

### 已证实的首败及有限成功

原窗口：[critical-path-wave18-c4f869af46ef](/workspace/zil-push-main/out/runs/critical-path-wave18-c4f869af46ef/emission.json)。binding中源码/tree如上，SDK DSO=`785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2`，LLVM_rebuilt=false。

`emission.log`：14 tests选择并执行，13过/1败、0 ignored；`completion.json all_gates_valid=false`。controller在首败停止，因此这里没有完整API/backend/driver/source/publication/strict/release通过收据。

失败定位：c4 [emission_assembly_tests.rs:163](/workspace/zil-push-main/crates/zil-llvm-api/src/native/emission_assembly_tests.rs:163)。`leaf("eax",0)` 有 `leal ..., %eax` 一处、`xorl %eax,%eax` 两处；canonical+clone共6处，断言期望4错误。6本身符合“寄存器不重命名”。修正应保留该例和 `%copy` 不得出现等完整断言，不能删除失败测试。

同一原log明确通过 `current_machine_bodies_keep_complete_ordinary_object_and_cancellation`：c4测试源码要求12函数、3类实际机器producer、9次复用、fallback=false，**完整 object bytes==ordinary**，原preflight receipt能绑定完整输入、pipeline=default<O2>、verify_before_after=true；立即取消拒绝与随后重试完整对象相等也通过。这是非零真正链路成功，不是只测parser。但取消回调为恒true，只证明入口取消；不能据此称覆盖中途ASM失败、取消时的全部阶段恢复。

## 2. 授权、输入与机器上下文

- driver [session.rs:1367](/workspace/zil-push-main/crates/zil-driver/src/session.rs:1367)：仅push_reuse、LLVM API、O2、未启用昂贵native-products、非early/late closed时启用。BuildOptions默认false；普通用户路径没有无条件打开。API入口是新的StructuredEmission，不接任意LLVM text/LLVM handle。
- 当前独立V位于session.rs:1227附近，仍实际 `typed.verify(candidate,...)`；后续Verify能力进入lower/backend原链。新选择未改Source、默认budget、LIR绑定或语言验证。API native.rs:639仍经过完整`validate_for_builder`、Builder、target/DL检查、LLVM verify、原完整`LLVMRunPasses(default<O2>)`和post-O2 verify，之后才进入try_emit。
- native.rs:797再次限制Linux x86_64、O2、merging Disabled；backend晚闭合/native-products分支优先于emission。新API传native_attempt=None，不将新机制与SDK多stage恢复scope混用。O0、其他target或merging不满足时退普通路径。
- emission.rs:70先检查External/default visibility/default DLL/C calling convention、非variadic、整数返回和参数（1/8/16/32/64，参数<=8）、4BB/64inst，然后才print。4096函数/4MiB累计打印key、单函数16KiB/128行是额外可选选择上界；超界回原路，不减少原输入验证。
- emission.rs:114白名单已生效：function/return/param全部属性按(index,kind,value)排序完整比较，未知enum、string、type均拒绝。故先前“未知enum其实仍接受”的施工问题在c4已修正。字典/别名仅本次不可变optimized Module内部；key使用完整字符串比较，不依赖hash碰撞，不跨CGU/request缓存LLVM owner。
- closed_key仅替换定义头中的当前函数名，保留签名、SSA/CFG文本、alignment等可见属性；拒绝metadata、ptr/内存相关形状、phi/switch/indirectbr/inline asm等，所有body global名字只能是固定trap/整数overflow intrinsics。Intrinsic定义/签名首先受当前LLVM verify约束，不能单靠拼写伪装一个任意外部函数。

**callsite边界：** typed tuple目前不遍历CallBase，函数打印中的callsite属性组未另取实际值。既有native/work_products.cpp:379的`projectFunctionText`会显式补齐complete-call-attributes，说明不能把这里称为通用LLVM函数相等证明。不过，本入口的model::Attr只有zeroext/signext/nonlazybind/noinline，preflight限制合法位置；Builder给call只添加return/arg ABI属性，没有function-index call属性构造器。可复用叶又只调用固定intrinsics，任意string/type callsite属性没有已确认的源输入构造路径。因此本审不凭理论裸LLVM反例判当前实现wrong-code。要进一步闭合，应逐instruction typed绑定允许callsite属性，或精确证明固定SDK产生的全部相关组受完整key约束；扩大raw/metadata/任意call域前必须补，不应靠“同module”一句授权。

**未打印的machine状态：** 旧native product显式绑定owned use sequence、block/name状态；此实现不绑定。不同机器寄存器选择可能仍实现同一LLVM语义，但破坏byte-identical目标。需要带相同print/不同构造或use-order的有效反例，保持比较完整object/ELF；不把未知差异统一归为metadata，也不提前断言一定错码。

## 3. Clone、ASM重构与重新组装

emission.rs:407从完整post-O2 Module clone后，仅对选中的duplicate改available_externally；原Module不经投机CodeGenPrepare，原linkage从未改动。canonical仍实际运行完整原MachinePipeline；duplicate保持其自己的函数身份，未把caller改到canonical，也未做runtime alias/ICF。clone范围扩大到完整module，保留当前调用/全局背景。

emission_assembly.rs逐段审核结果：

- `parse`跟踪text section；拒绝宏/条件汇编/符号别名/语法和架构切换等能隐藏定义的指令。解析出的普通定义序列必须与原IR inventory减去duplicates完全一致，不允许少函数、额外定义、重排或canonical也是alias。
- `Template::new`只允许当前text下的global函数头、p2align/LLVM23 prefalign、函数体和精确`.size end-owner`；CFI、EH、data/section、未知directive均拒绝。只接收完整当前producer实际ASM，不接磁盘用户拼出的模板作为授权。
- 复制体只允许明确的整数指令、寄存器与本体内.LBB分支；call/外部symbol/RIP/未知symbol拒绝。栈访问限定rsp/rbp；被复制函数内没有需重绑定的外部relocation/CFI。普通函数可以有CFI与外部call，原bytes运输，不被误当可复用模板。
- replacement按完整token范围，`%eax`作为完整寄存器token，不会被函数名eax替换；`@function`不会被owner=function误改。局部标签定义/引用、size与prefalign端标签一起重命名，clone前缀在完整原ASM中出现即拒绝，ordinal保证新标签相互不同。
- clone插回原IR定义顺序，放在完整普通函数CFI结束之后、后续globals/section切换之前；非复制函数及其数据/directives保留。不是把新体统一追加到文件尾部，也不是手动改ELF偏移。
- module_file只删除一个位于首函数前的unnumbered `.file`，编号debug `.file`保留；重复/错位/不完整模块record拒绝。这是解决empty-module自行产生STT_FILE的明确策略，不是最终ELF归一化。源码文件名从当前source Module复制，字符串边界有限额。

emission.rs:341建立新LLVM Context+Module，安装私有error diagnostic handler；ASM返回非零/空buffer/超过32MiB或诊断severity Error都拒绝，最终Object buffer通过RAII释放。最终assembler重新计算布局、alignment、relocation和保留普通函数的CFI，避免手写ELF拼装风险。

**适用边界而非泛化保证：** assemble没有显式复制DataLayout/module flags。当前TM固定x86-64/空features/PIC/default model；structured Builder创建zil-unit且当前model没有module flags/debug/inlineasm字段，此有限来源约束与新context、完整ASM运输共同限制了环境。不能推广到任意带flags的LLVM module。建议明确设置同TM DataLayout，并对未知module envelope一次拒绝，避免未来表示扩展默默扩大权限。现有12函数完整object相等证明已覆盖该固定SDK的基本`.file`/symbol/layout往返，尚未覆盖所有混合普通函数/全局/CFI组合。

## 4. 拒绝、取消、缓存与发布

Selection miss直接普通emit；clone/verify/ASM/parser/assembler失败记fallback并回原Module。check(cancelled)在选择后、投机ASM后、重构后、重新组装后执行；取消传播Error，不降格成成功fallback。所有投机object/ASM都在内存/private owner内，失败不进入PendingObject。原Module保持post-O2状态，可重新进行普通Object emit，不在已被CodeGenPrepare改动的clone上仅恢复linkage。

native.rs:796起的emit计时包围选择、试验、重构、组装及ordinary fallback；因此失败尝试不会从总emit/total_ns中被漏掉。分项统计不包括所有clone/verify固定开销，但总计包括，不能把分项和当总计。只有组装成功才设置reused_functions>0；missing selection不是“成功复用”。

backend成功后才写staging object，构造PendingObject；driver在最终取消检查、publication commit之后才commit_object_cache并采用候选backend session。未观察到新分支绕开已有事务发布。原cache hit仍需完整当前machine preflight/精确checker receipt，不能继承任意旧验证权限。

**cache身份注意：** PartitionInput和final-product Configuration整体持有BuildOptions，新增flag参与内存配置比较；disk PreparedCache::new_with_policy则没有emission_reuse，namespace仍绑定compiler/LLVM binary、target/pipeline/model及原策略。故同binary、同machine输入、同cache根下，开/关emission可以共享对象。若新路径已满足普通object字节相同合同，这是可兼容设计；在验收尚未闭合前，它会掩盖实际producer或把试验产物带入“关闭”控制臂。请用独立cache根或fresh API进行on/off质量对照，若必须支持强隔离则显式纳入策略版本。不能将一个cache-hit运行当当前非零复用实证。不同compiler binary仍已被namespace隔离。

## 5. 必要的下一批收口；不扩大无关重跑

1. 修正上述唯一已证实测试期望，保留c4首败；新SHA先跑受影响emission组。既有完整object相等的成功用例不能仅以“整个组失败”抹去，也不能冒充组通过。
2. root正在实现的ConstantRange**不属于c4**。c4的非enum拒绝应保留为首版保守边界。后续只允许明确kind=range且index为返回/实际参数，完整打印值必须确实包含位宽、lower/upper；未知非enum绝不概括为range，function-index仍走原typed白名单。固定SDK Core.h有CreateConstantRangeAttribute，但未声明IsConstantRangeAttribute/范围getter；须核对采用的kind访问API，不对range调用GetEnumAttributeValue。
3. 新range批必须实际rem251非零复用+full ordinary object equality；范围端点、wrap-around区间、位宽、位置变化要使不同key/拒绝，不能仅测试普通add叶。不要用unknown拒绝改成“忽略属性”来追求命中。
4. 集中补新链路最相关的反例：混合不可复制CFI/global/caller与可复制叶、不同ABI属性、强制投机parser/assembly拒绝后原ordinary相等、中途取消后无产物发布/再次正常编译。c4只有入口取消，不称这些均已实测。
5. 新SHA依照root统一窗口继续相关API/backend/driver/source/publication/strict/release；真实2500原输入16unit/fullO2/currentV/defaultbudget/完整ELF正式门原样保留。1000 research/stock旧首败与comparisons空不变。本审没有修改质量门，也没有给新body复用授予机器差异例外。

## 6. 精确源码摘要

以下为固定Git blob内容SHA256（不是工作树新修改）：

| 路径 | SHA256 |
| --- | --- |
| native/emission.rs | `b26186bc288fca9cbb1fb95da7c8d1b4f187163ed6a4b0ea603127a05586a544` |
| native/emission_assembly.rs | `580c846cdbe143e92eeab2f80054dc40dc03d07257e9df243b20e36e86b4914e` |
| native/emission_assembly_tests.rs | `5ece238cb27ac682af8b1536ff139ec02706aff1935dab3ee622b5fee4a942eb` |
| zil-llvm-api/src/native.rs | `df2a32aed20e651b593fed120d0c093d489c87c2dcf8e5f9eaffaa8289b1e430` |
| zil-llvm-api/src/lib.rs | `5df31f688c3ec7315401c7bb6fd795db51a92f74fa3164f3028a614340b3b625` |
| zil-backend-llvm/src/build.rs | `07b81f0e60f2db1a964f56a5ae32392e53bd9a81422886c34c4dbe4efaba376a` |
| zil-driver/src/session.rs | `629dfce2bd63b1c226eff9c5a0de6668a384848a56ac532272d997c080ee82a1` |

本次给出精确源码和有限运行收据的独立复核；没有声称新完整链全部通过、真实大项目已提速或后续range修改已接受。

## 补充：交付前出现的 fca282b range 增量

另读固定 `fca282b2103846ec1fa82099b2616b520b932002`（父提交正是c4），只改emission.rs及ASM测试，共51增/3删。**此增量的源码方向认可，运行验收尚未在本审看到，不改写上述c4失败身份。**

- 新识别先拒绝null/string/type，再取kind；非enum分支必须kind等于固定SDK识别出的非零`range`，且index不能为function-index。索引迭代仅0..=实际参数数，因此域限于返回/真实参数；其他未知非enum仍拒绝。不是把所有非enum视为range。
- range只把(index,kind,0)放typed tuple，0不是范围值；实际上下界/位宽必须由原完整function打印key绑定。新测试有inline return range端点变更不等；对于当前已verify的primitive签名，这个分工成立，不需要为范围虚构enum integer value。未调用GetEnumAttributeValue读取range，避开了该representation误用。
- 安装LLVM23的`llvm/IR/Attributes.h:270`明确`hasKindAsEnum()`适用Enum/Integer/Type/ConstantRange/ConstantRangeList，排除string后取kind符合该底层契约；这不等于SDK提供了不存在的IsConstantRangeAttribute C API。实际CAPI链仍应由新的rem251 native测试收据验证。
- 原12函数原生测试改成 `(arg + i%3) srem 251`，保留完整object equality、9次实际复用/3body、fallback=false、取消重试等断言，方向满足“必须覆盖实际range生产者”。端点文本反例是有效key检查，但不能冒充已经执行typed未知属性/错误位置的完整原生反例。
- ASM测试改为去注释后按完整`%eax` token计数，要求原计数非零且输出严格翻倍，同时保留copy标签及不得出现%copy，合理修正旧substring数量期望，没有降低相应语义约束。其余ASM生产代码未改，可复用本报告对应源码审查。

后续仍保留主报告的callsite/未打印machine状态、混合module运输、缓存控制身份和中途失败覆盖边界；range适配没有扩大这些权限。新批运行通过前不宣称fca已整体验收。

## fca282b 再核与 wave19 首败定位

按用户最新要求重新执行只读`git rev-parse HEAD`：仍为`fca282b2103846ec1fa82099b2616b520b932002`，工作树干净，相对c4仅上述两个文件。range增量源码判断保持：明确kind/位置授权，完整inline值进入key，没有对range调用EnumValue accessor。另读root保存的`post-o2-current2500.ll`，确有`define range(i32 -250, 251) i32 @zil.fn.5...`等实际定义；root报告的kind=104、enum/string/type=0与此适配一致。本人未执行该LLVM探针。

**新运行不能标Pass：** [wave19 api.json](/workspace/zil-push-main/out/runs/critical-path-wave19-fca282b21038/api.json)绑定fca/tree `e569577b1b7811242617eee4caa945301ce6c7fd`、相同v18 DSO。全API **97过/1败、0 ignored**，退出101，controller已停止，backend/driver等后继未执行。唯一失败为新版rem251原生测试emission.rs:579的完整object equality；旧c4的13/1首败仍单独保留。

只读解析新api.log断言里的完整两份byte数组为ELF64，不运行编译/assembler/disassembler、不改原证据，结果如下：

| 项目 | ordinary / reconstructed断言值 |
| --- | --- |
| 对象长度 | 均1456B |
| 全对象SHA256 | `d43cdd43cd373db32c7004525cde7f4cb34757e71ee835bfc4efb306442df4be` / `11f7d02873a9f3b11c944480adec9c22a75df6464817433a7f595bb719ba6766` |
| `.text` | 均file offset64、长度570B；36个差异字节全部在此 |
| `.strtab`、`.symtab`、节表及其余字节 | 相同 |
| 12个函数symbol范围 | 地址/长度相同，12份body各自逐字节相同 |

差异精确位于`zil.fn.0→1`、`3→4`、`6→7`、`9→10`四个函数间的13B alignment gap（file offsets `0x63`、`0xf3`、`0x183`、`0x213`），每处9B不同：

```
ordinary:     66 66 66 66 2e 0f 1f 84 00 00 00 00 00
reconstructed:66 2e 0f 1f 84 00 00 00 00 00 0f 1f 00
```

这是同长度的13-byte NOP与10+3-byte NOP填充形状差异，**不是range属性导致函数运算体变化，也不是ELF元数据差异**。它仍违反当前完整object字节合同，不能归一化或豁免。此用例12个body一致，也不等于所有地址可观察/运行质量合同已自动完成。

具体修复优先检查普通object emitter与empty-module inline-ASM parser所用的**MCSubtargetInfo/NOP长度与填充策略**是否一致；安装头文件`llvm/MC/MCAsmBackend.h:180`的`getMaximumNopSize(const MCSubtargetInfo&)`明确该选择依赖STI，单有同TargetMachine/triple尚不足以据此排除差异。现有证据定位到填充结果，尚未证明哪一个feature/选项是首个原因；不要在本报告基础上直接猜一个target feature或重写最终ELF。用零复用13B gap对照闭合运输设置，再跑相同rem251门。

注意源码顺序：完整object equality在stats、reused9/fallback断言之前，本次失败没有执行这些后续断言，不能把c4成功的reused9/cancelretry收据移用于新版rem251。原log SHA256=`165f499e2b57af84cf548b901278088e3921d104e9428d2e2b5a85bf63b929d5`。fca整体继续Hold；当前已提供足以定点修复的具体产物差异，不建议先扩大重跑全套。

### 保存对象的独立 ABI/指令复核

后续只读`readelf -W -S -s -r`与`objdump -dw`核查root保存的[ordinary.o](/workspace/zil-push-main/out/runs/emission-byte-diagnostic-01/ordinary.o)、[reused.o](/workspace/zil-push-main/out/runs/emission-byte-diagnostic-01/reused.o)。两个SHA逐一匹配上表的首败数组，未运行对象、编译或assembler。

此对照**没有发现ABI或函数计算语义差异**：12个GLOBAL/DEFAULT函数符号、STT_FILE zil-unit、全部section headers相同；两文件均无relocation，也无eh_frame/LSDA。每份函数体逐字节相同，整数参数仍取edi、结果仍写eax，中间仅使用caller-saved rax/rcx/rdx，没有新增栈操作、callee-saved寄存器修改、外部call或分支。每个函数以ret结束，四处不同填充均在ret之后、下一个函数入口之前，正常按符号入口调用不会执行它们。

objdump确认普通路径为单条13B多前缀NOP，新路径为10B NOP加3B `nopl (%rax)`；后者的内存形式操作数是NOP编码，不是新增实际load。差异包括canonical zil.fn.0之后的间隙，所以不是仅duplicate局部标签替换才触发的问题，优先留在root的ASM→Object运输/填充配置诊断。以上结论仅针对这一对已保存对象，不外推其他输入，也不改变完整字节质量门的失败裁决。


## 最终增量复核：d71 carrier → 5a mixed carrier（2026-11-14）

结论：**接受冻结5a的有限实现与组合运行证据**，未发现此增量内新的阻断性授权、ABI或ASM语义问题。此结论不替代真实大项目完整ELF/性能门，也不追溯改写c4、fca、d71首败。复核期间只读源码、保存产物与收据；没有运行LLVM、compiler、build、tests或benchmark，没有改生产或Git。本节由用户明确授权追加。

固定身份：

| 源码 | Tree | 范围 |
| --- | --- | --- |
| `d71ce38e556b2a8e5cfb7b235ad54f8ccd787ef3` | `beb8b21e54e4fe08e2d4c7b7860d3a19f18606be` | 相对fca只改emission.rs，123增/2删 |
| `5a1434573ecd2ce8aa233e6841e9181ff3d9fd9d` | `5fa75d9baeee63e30c6e6f9cfb38f4b0c4c0409d` | 相对d71只改emission.rs与emission_assembly.rs，135增/8删 |

本次HEAD仍5a，检查时工作树干净。5a emission.rs SHA256=`2469714a3556b8eb36943bb783a5c64f104216d6aa46321d8572e0c4f4fc90f1`；emission_assembly.rs=`3721d39b64aa3fd556565aeae0aaefa45918e7252f69ae495d9158e02da720e1`。

### Carrier 与上下文

emission.rs:399的assemble改用新私有Context中private+naked+nounwind void() carrier，side-effect inline ASM后unreachable；复制当前SourceFileName、triple和DataLayout。作用是让AsmPrinter的inline ASM解析获得函数STI，恢复同target的长NOP选择；没有硬编码NOP、重写ELF或归一化失败。payload内保留每个实际函数独立symbol/label。reserved carrier字符串碰撞即拒绝；所有美元号逐字节双写，避免被函数inline ASM当operand；尾部恢复.text，避免carrier结束标签落入数据或GNU-stack节。长度算术与buffer限额、私有错误handler、整次fallback均保留。

原post-O2 Module不变，available_externally与CodeGenPrepare仍只发生在独立speculative clone。carrier解析/组装失败丢弃全部临时产物，回原Module ordinary object；取消传播，不发布partial。fca的range只允许已识别ConstantRange且位置为return/param；完整inline范围及typed index/kind参与key，不调用EnumValue accessor，未知/string/type/function-index range继续拒绝。

5a把diagnostic bool声明移到Context之前，保证Context及其子owner析构时payload仍存活。三个getter返回c_uint与已有声明统一；它们是enum getter，所用已知零/非零判断与旧32-bit c_int表示等价，不应称这些enum只存在0/1。ASM if-let折叠不改变拒绝条件。

先前零复用carrier探针保存的ordinary/carrier均1456B，SHA256均`d43cdd43cd373db32c7004525cde7f4cb34757e71ee835bfc4efb306442df4be`；这是运输对照，不冒充非零复用证据。d71真正rem251 native用例通过完整object equality、reused9/fallback=false及取消重试；5a进一步覆盖混合模块。

### Mixed carrier 与取消

5a原同一个native测试增加活global emission.data、外部声明emission.external，以及NoInline普通函数zil.fn.90：load global、真实external call、合并返回值。检查ordinary包含.eh_frame/.rela.text及两symbol名，复用后要求**完整object逐字节相等、reused9、fallback=false**。字节串存在断言不是独立ELF解析，但此用例确实将不可复制普通函数的CFI/data/relocation运输纳入相等断言。不会把该普通函数错当闭合叶。

按实际成功callback次数total，在total−2触发取消。结合try_emit组装之后的check与native.rs最终emit之后的check，此处定位到after-ASM检查；随后重新编译要求ordinary bytes相等。不是仅入口取消。当前证据覆盖这一混合fixture，不泛称全部debug/module flags/特殊target设置均已覆盖。

### 运行收据组合及边界

以下均实读现有JSON，不是本审重新执行。目录为仓库out/runs/critical-path-wave20-d71ce38e556b与critical-path-wave21-5a1434573ecd；两者绑定同v18 DSO SHA256 `785a2413d05149ba84789a588f5ca2d4d6b1ef34248ca282789acd4f3133c1d2`，LLVM_rebuilt=false。

| 窗口 | 实际运行结果 |
| --- | --- |
| d71 wave20 | emission14/14；backend273/273；driver219/219；Source9/9；publication1/1；strict失败，completion=false |
| 5a wave21 | emission14/14（含新增mixed/after-ASM断言）；strict、fmt、release均过；completion=true |

5a emission日志SHA256=`60fded431c2511eb5c634952c8fdb02491bed07afb434f4975f5a0459a6f49f5`；release.json记录binary SHA256=`54815d6064473ec9cde849f8fc2b8573abeb9de0a0e2e5d724142d08c94a32fa`。该binary哈希此处为收据值，未另作运行。

复用d71 backend/driver/Source/publication收据有明确差异依据：对应消费者源码未改，生产差异仅getter声明签名修正、handler生命周期修复及等价if-let，受影响emission和strict在5a重新实际执行，另新增混合模块及after-ASM反例。故接受“5a当前受影响检查 + d71未变消费者”的组成证明；不能写成5a重跑全套，不能将14测试中的扩充断言再累计为额外测试。fca全API97过/1败中的失败由后续受影响用例闭合，不改写为fca全API通过。wave21的completion只覆盖该窗口计划，不代表所有产品门或wave04正式性能窗完成。

## 后续方向：earlySSA baseline兼容与大体选择（方案复核，非5a实现）

已读Gauss报告 `/workspace/zil-recovery/native1000-full-elf-diagnostic.md`。保存1000的stock与push对象完全一致、只八个aggregate真实机器体变化；aggregate原定义相同但被内联helper从alloca/load/store+guard变为direct SSA+phi，差异早于link。它支持检查emit/typed_scalar_facts.rs:281的finish_scalar_ssa，以及emit/early_scalar.rs:42、:123附近SSA/range/cleanup与guard/PHI incoming/block/operand顺序，**不支持目前就判定某一个pass或顺序修复必能恢复baseline字节**。

有限可证伪路线：只围绕相同aggregate及其helper闭包，利用已有中间证据寻找第一个差异；若只是等价构造顺序造成LLVM tie-break改变，在新SSA producer保留baseline兼容的块、前驱、PHI incoming、operand与guard构造顺序，可保留不再建临时storage的宿主收益。若确为必要的guard/CFG形式差异，可另候选在结构性不兼容的完整primitive域保留旧lowering，而非按八个symbol/模13特判；但当前没有足够证据界定该域或承诺相等，不建议现在实施盲目的选择性回退。优先保留已有效2500机器门与稳定IR的宿主/emit优化。旧1000窗口仍失败，不能用新复用链声称已修复它。

保存示例.28为8481B/202行/29BB/143inst；.53为8089B/193行/27BB/138inst。只读统计post-o2-current2500.ll的101个超旧4BB或64inst函数，去掉各自定义头函数名后有101个不同完整文本key、零重复组（这是全module文本观察，不是实际CGU命中预测）。因此仅把optional selection提高到128BB/512inst/512行不会解决local BB名字不同；16KiB单key、4MiB累计与完整Source/Verify预算是不同边界，不能混称提高源码预算。

对用户随后提出的**独立print-only LLVMCloneModule alpha规范化**给有界方案认可：先按原函数typed签名/BB/inst范围筛选；在print clone对实际参数、BB-asValue、指令清空名称，让LLVM自身分配slot并打印；保留所有global符号、常量、属性、CFG和顺序，只另去定义头自身名。严禁该clone参与codegen或替代原始/speculative producer；全module内identity映射必须按未变global名核对，typed属性与原函数一致。clone失败/限额/未知属性按既有路径miss/fallback。局部名称可忽略，不等于可忽略未打印use-list等机器上下文，fullobject/fullELF门仍必要。

实施前最有意义的native fixture：至少五份同大体、不同参数/BB/SSA名字，经过完整O2后仍>64inst且>4BB（不能靠会被O2折叠的直线add链），采用真实sadd.with.overflow→trap分支和srem链；单独要求这些大体产生至少四个duplicate、nonfallback、ordinary完整object相等，避免小叶的reused数掩盖大体miss。同时一份只改常量/分支目标/返回range端点的最近反例不得同key；保留混合CFI/global/external与after-ASM取消重试。机器spill/callee-save/长短分支/局部alignment应自然进入该实际大体；若产生CFI或未知指令，现有模板应整次拒绝并回ordinary，不放松门。大体新触发整次fallback可能连小叶收益也损失，需从真实stats辨认。

性能边界：新增print clone/逐value清名/打印成本必须计入selection和总emit。扩大复用节省的是machine_assembly_ns中的重复机器生成，完整最终ASM仍全量重新组装；若root所说699ms实际为assembly_ns，不能期待大体复用消除这699ms。必须区分这两个现有字段，不将首次909ms与旧1218ms当正式比例。当前冻结5a/wave04与后续alpha/大体新SHA分别保留身份及完整ELF正式门。


## 76993726b 精确增量独审：print-only local alpha

固定源码 `76993726b2e986ccd49b50c6424748d335056aeb`，tree `13f8178981ea855fb20f5459b806b09ec51fcb3b`；相对5a仅native/emission.rs，224增/4删。检查时HEAD相符、工作树干净。该Git blob SHA256=`c60eb889f122f200171c051d59fb5911f0793d79910f86467f63d059f7f51e41`。结论：**新增实现的有限源码范围接受；未发现阻断性FFI、owner、key授权或fallback/publication问题。** 运行证据截至本节实读为wave22 emission15/15、strict/fmt通过，release/completion收据尚未出现；正式Source05/fullELF尚不在本节接受范围。只读现有源码/头文件/日志；未自行运行LLVM或构建测试。

- emission.rs:24–26新增LLVMGetParam、LLVMBasicBlockAsValue、LLVMSetValueName2，与安装LLVM23.1.4 Core.h:3348、3700、2161的参数/返回类型相符：unsigned索引、opaque handle及size_t长度。实际参数索引来自已验证非vararg函数的类型参数数；BB先转Value，指令含void terminator只清空名字，不试图为void赋非空名字。传递静态空C串，未传悬空缓冲。
- :320 clear_local_names仅获print_clone内按未改全局名查得的函数。遍历参数、BB、指令清名不改变def-use/CFG或容器顺序；next迭代不受名字删除影响。:366懒建一次完整LLVMCloneModule，同原Context但独立Module/函数/参数/块/指令owner。select所有出口均RAII销毁print clone，Selection只保有String；其生命周期结束后try_emit才另建机器producer clone，没有把print clone或其裸handle泄漏给codegen。原函数全局名保留，字节串复制在owner存活期内完成。
- :386 key由print clone完整LLVM函数打印、仅替换定义头自身名，再拼接**原函数**typed属性；所有全局引用、常量、范围、签名、属性组、块/指令顺序继续绑定。局部slot由LLVM统一重编号，不做regex引用替换。clear_names不改属性，原/clone属性对应成立；lazy清其他函数的局部名不改全局symbol或属性内容。闭包、未知/string/type拒绝、return/param ConstantRange与ASM严格运输门均未改。SSA参数交换、operand、branch target或常量不同仍会在规范化文本中表现为不同关系，非泛化的语义等价优化。
- 新128BB/512inst门在打印前由原函数遍历检查；512行/16KiB单打印、4MiB累计打印限额保留。4MiB不是整个clone或总内存的上限，也未计typed tuple串；这是原有计数定义，不可扩大宣传。第一次遇到候选便clone完整Module（未先完成全module计数或属性/闭包筛选），所以selection_ns新增完整clone/清名成本，需看实际净收益。
- clone失败、查名失败、累计超限或无足够alias均返回None，原Module ordinary emit；这些发生在selection内时stats.fallback仍false，不能只用fallback=false证明成功，必须同时reused>0。后续speculative/ASM失败、取消、原Module fallback及发布逻辑未改。select本身无取消callback；新clone/规范化期间取消需到selection完成后才响应。若select返回None，按现有路径到ordinary emit后的check才响应；不会成功发布取消产物，但不是逐函数即时取消保证。总emit及selection计时包含新增工作。

新增native测试:673以12函数/3常量形状、不同参数/BB/SSA名，32轮add→mul→srem251产生真实长体；用完整O2+前后verify后实际计数断言每体65..512inst，证明未被优化成小demo。随后要求9reused/3distinct、fallback=false、完整ordinary object相等；逐个原函数打印前后相等并再次verify，覆盖原IR和名字未被selection/speculative producer污染。常量3/5/7三组仍distinct，提供真实差异key反例。这个测试直接调用try_emit配合独立普通API oracle；入口API及mixed/after-ASM取消路径由同轮原有测试覆盖，不能将此新测试描述为完整Source验收。

**剩余具体覆盖边界：** 新fixture只有一个BB，没有conditional branch或trap；它证明长SSA/参数/entry名称alpha，尚未实际覆盖新>4BB域的分支引用及多trap块复制。建议Source05用真实aggregate非零命中+完整ELF闭合该域，或在同一native fixture中追加O2后仍>4BB的overflow/trap体和一个分支目标不同反例；无需重跑不相关全库。若实际大体触发CFI/未知ASM导致整次fallback，应记录并保留ordinary；不能为了命中削弱模板门。alpha只取消局部名字身份，不绑定所有未打印machine/use-list状态，完整object/ELF质量门继续必要。

wave22目录`out/runs/critical-path-wave22-76993726b2e9`绑定相同v18 DSO、LLVM_rebuilt=false。emission.log明确新long_current_bodies测试ok，总15过/0败/0忽略（84 filtered out），日志SHA256=`a74aa257fa8f58341f9c517a683b8012f44b6edea88450ba1aae40cf57698146`。strict/fmt均valid=true。原backend273/driver219/Source9/publication1可作为未改消费者与事务/配置接口的组合证据继续引用：这些源文件和调用合同未变，受影响emission链本轮实际重验；但它们没有在769重跑，也不独立证明新增alpha的大型CFG机器质量。5a batch04与本SHA后续Source05分别保留身份，旧首败不改。


## 05 wide 实际命中负例：扩大 alpha 域没有增加复用

本节只读 `out/runs/large-project-large2500-{04,05}-o2` 的pins、rows.jsonl、逐请求response/pair-validation、对象inventory与归档LLVM，以及当前machine_cgu.rs和保存的输入生成源码；未运行compiler/LLVM/build/benchmark。05绑定76993726b完整SHA，binary收据SHA256=`49231ac119ce5aa9cdf352a05e1c485cfdfb5950fa63959081f0e564e8eb0cfb`；04绑定5a及其54815d...binary。wave22 release/completion现已实读valid=true/all_gates_valid=true，闭合上一节当时未完成的记录，不改写其时间状态。

04、05 wide均完整102行/34请求对、valid=true；逐pair实读各32个成功原生对executable_equal=true，另2个预期error对没有executable_equal字段，均valid=true。完整输入/defaultbudget/currentV/fullO2及完整ELF门保持。两块cold的04/05 source_identity相同，按unit_index逐一核对16对象payload SHA256也全部相同。

**root假设成立：跨module有重复形状，当前同CGU内没有aggregate重复。** 归档原LLVM的main先调用输入函数zil.fn.3，再按模块0..99调用zil.fn.(28+25*i)，从而能将实际对象defined_functions反向绑定到模块，不凭symbol数字直接猜归属。machine_cgu.rs:457按(file,chunk)合组，:480按(file+chunk)%count分桶；该输入每文件25定义、没有大文件拆块，actual inventory吻合owner=(i+2)%16。实际unit2..5各7个aggregate，其余12unit各6个。

例：unit0含模块[14,30,46,62,78,94]，模13为[1,4,7,10,0,3]；unit2含[0,16,32,48,64,80,96]，模13为[0,3,6,9,12,2,5]。全部16单元逐一检查均无重复余数。保存的implementation-large_project_matrix.py:117给cold helper常量 `(i+j)%13+1`，wide无module依赖；23个helper的有序算式由i%13决定。同CGU相邻module差16，在mod13上每次加3，重复周期需13个module，而每单元仅6/7个，故该全局13形状不能在当前unit内相遇。此解释针对实际wide输入及分组，不泛化到chain/diamond、其他source顺序或其他大小。

两块cold的计数均精确如下：

| 候选 | considered合计 | distinct合计 | reused合计 | fallback单元 |
| --- | ---: | ---: | ---: | ---: |
| 04 / 5a | 2401 | 308 | 2092 | 0/16 |
| 05 / 769 | 2501 | 408 | 2092 | 0/16 |

即新增100个considered、100个独立body，**新增实际复用为零**。这也与2300小helper、每CGU13个canonical（共208）、2300−208=2092次复用吻合；100个aggregate没有减少自己的MachinePipeline生产。此处不是猜测alpha实现失败，而是扩大域的实际负收益机会记录：能力正确进入候选范围，但该分区下无重复对象可省。

进一步逐一比较两窗34个candidate请求的fresh miss对象数/reused/fallback三元组，全部一致；每窗合计82次fresh对象生产、11032次函数复用、0fallback。统计只取cache.state=miss，避免重复计算cache hit中的旧receipt。这将“没有额外命中”扩大到这两窗完整wide请求序列，而不只第一cold。

| cold诊断（ms） | selection | machine_assembly | assembly | emit总计 |
| --- | ---: | ---: | ---: | ---: |
| 04 block0 | 47.479 | 699.012 | 106.805 | 909.227 |
| 05 block0 | 100.684 | 629.563 | 96.037 | 875.523 |
| 04 block1 | 49.697 | 638.118 | 99.934 | 839.219 |
| 05 block1 | 82.710 | 879.373 | 97.107 | 1100.789 |

这些是不同正式窗口的已有诊断切片，**不构成04→05受控性能比，也不据此宣称提速或确定回退幅度**。它们确认先前699ms是machine_assembly_ns，不是纯重组装assembly_ns；选择成本值得控制，当前扩域没有增加任何省机器生产的实际命中。两个block的emit方向也不同，不能只选875<909作优势。

建议保留769的完整功能实现/native反例与fullELF成功证据作为研究能力；不因该负例删除通用大体能力，也不将其宣传为当前2500优化收益。自动选择宜保留小叶廉价路径，昂贵print-clone/alpha/wide仅在**当前CGU**出现低成本重复候选时进入：先取alpha不变的typed签名、BB/指令数及必要opcode/常量结构摘要分桶，没有同桶pair就跳过该扩展；摘要只控制是否值得尝试，永不替代完整规范化key、typed attrs和原ASM门授权。发生碰撞只会多付尝试成本，最终仍精确key确认；即便选择保守miss也必须完整ordinary生产。小叶已有重复时不能让无重复大体强制承担全部扩域打印；是否付完整clone成本要有实际候选依据。不要按模块编号、mod13或本例名称特判，更不要把“全程序13种形状”当同CGU预计收益。

本批无需跨CGU复用或改变partition来制造命中；两者会引入独立机器上下文/布局/增量失效问题，不是现有证据要求的必要修复。完整更广功能链目标继续保留；后续若研究自动选择，按同输入独立对照证明净秒数，新增质量域保留完整ELF硬门，1000旧失败不改。
