# 下一条 native 关键路径：Astra 只读复核

结论：优先实施 **完整 O2 后的封闭叶机器体复用＋保留原布局的 object 重建**；若纯 executable 的无外部导出合同明确，则 **完整 O2 后的死机器体清除** 是另一条更直接的路线。不要再把本窗主因归到重复 IR O2、全量 CGU 声明或 LLVM 文本 assembly。跨 CGU body import 暂不列为本窗第一优先。

绑定：checkout `research/full-push-native@4b3a99b873b433501fb9c24184b1348023316b11`，只读 status 干净；实测 compiler `1de00f63f80cf4534ec995dc51e106005b270ddb` / binary `4d040bb3dea0d3ca8b4ec06e38d47c359289a74c54ab3069b7bc90fd9f0fa5f7`。两 SHA 的 backend/API 路径 diff 为空。已读 AGENTS、checkpoint md/json、goal/workflow。只读现有源码、JSON、LLVM/ELF bytes，没有 compiler/build/benchmark、采样或 Git mutation。唯一写入本报告。

## 现有证据足够支持的判断

原始窗口：[large-project-large2500-02-o2](/workspace/zil-push-main/out/runs/large-project-large2500-02-o2/summary.json)，valid=true，102 rows，2 blocks；数据取其 `rows.jsonl` 的 candidate/current object reports，非旧 receipt 重贴。cold 2.821429s，Rust 1.390107s，差 1.431323s；broad 2.364011s / Rust 1.180752s。

| 当前费用 | cold block0 / block1 | 本窗口每条17请求序列的两 block 均值合计 |
| --- | ---: | ---: |
| object emit | 1.217951 / 1.491657s | 3.433950s |
| IR O2 | 0.395015 / 0.391003s | 1.020732s |
| Builder | 0.026951 / 0.059131s | 0.080948s |
| preflight | 0.031386 / 0.171001s | 0.199061s |

每 block 是 cold16＋broad16＋8次private各1＋recovery1＝41 个 object miss；命中对象没有重复 O2/emit。这里是实际序列频次，不是工业用户权重；秒数是整项可优化费用域，不是已测可节省额。没有逐函数 emit ns，**不能按函数数或代码字节占比把1.35s分摊成收益**。machine_ns 的624/119ms跨度不作因果归因。

独立读 candidate cold 的16份原 `.o`：2504个定义（含 C main wrapper），2503个应用定义；2300个 helper 长41/42B，101个函数长6B；13个41/42B代码类别各重复176–179次。函数 symbol 范围总字节262905，不含函数间 padding，不能与 root 的 ELF `.text`278982B混算。此分类仅证明现有机器片段重复，尚不证明可安全替换或无 relocation。Rust **原始16份对象 inventory** 共有105个 T/t符号；与 root 最终链接 ELF 的符号计数口径不同，不相互替代。

具体样本：candidate unit0 `blobs/b27ee8e6355025299cea7d58c52bd007d4af508541062872d1aa50cd5768e6a0.o` 有 `.eh_frame`96B、`.rela.eh_frame`48B；unit1 `blobs/a94a55e7afb3099df07d6c3de52ee997cd471e62e67bc8df393bbdb80c7fbdbc.o` **无 eh_frame**，但 `.rela.text`48B。两者 `.text` alignment16。因此“全CGU无relocation”太窄，“叶范围无relocation即可”又不充分；需按函数及相关节的完整依赖绑定。

已排除/降级：

- [native.rs:725](/workspace/zil-push-main/crates/zil-llvm-api/src/native.rs:725) 一次 `LLVMRunPasses(default<O2>)`；[native.rs:778](/workspace/zil-push-main/crates/zil-llvm-api/src/native.rs:778) 一次 `LLVMTargetMachineEmitToMemoryBuffer(ObjectFile)`。后者包括机器流水线、AsmPrinter/MC，不是第二遍 IR O2，`emit_ns`也不能全部称为 assembler。收据全是 builder、parse_ns=0；program.ll 是诊断输出，非文本回灌。
- [machine_cgu.rs:606](/workspace/zil-push-main/crates/zil-backend-llvm/src/machine_cgu.rs:606) 已按实际 references 过滤 prelude；:624只引入需要的跨单元 declaration。:503一次完整 declaration-only preflight 防止未用坏声明被过滤掉。NamedType仍全带，但本窗原LLVM named type数为0。不是2505声明×16的旧设计。
- 当前 native_product_selection=false，reason=`below-minimum-average-lir-work`，实际41372 LIR instructions/2503 bodies；没有 native-products receipt。因此 [work_products.cpp:434](/workspace/zil-push-main/crates/zil-llvm-api/native/work_products.cpp:434) 的 worldText、[machine_products.cpp:2126](/workspace/zil-push-main/crates/zil-llvm-api/native/machine_products.cpp:2126) 的 producer World 捕获**不是本窗已付的大头**。不建议靠下调阈值恢复多stage捕获。
- 每对象新建 LLVM Context/TM 确实存在（native.rs:550附近）。cold object total减去preflight/builder/optimize/emit约14/16ms，包含创建、LLVM verify等，尚非context独占时间；不足以解释秒级差距。
- 2504个原LLVM definition全部external。源码每模块23个private helper＋value＋unused_checked；同模块helper已经能参与原CGU内O2。不要把“helper被保留发射”误诊为“helper缺跨CGU内联”。真正跨CGU候选主要是main→value，是否值得内联必须另证，可能增大O2/代码体积。

## 路线1：O2后机器体复用，保留所有符号与原object布局

对 root 新方案的裁决：**可实施，但不能把现有 object_patch 当成已完成的纯ELF拼装基础。最难的是布局/元数据复原，非hash查重。** 无需重建LLVM；在Rust API边界用现有LLVM C API检查actual optimized module、暂改重复叶linkage，再实现限定ELF重建器即可。不运行第二遍IR O2。

入口：[native.rs:744](/workspace/zil-push-main/crates/zil-llvm-api/src/native.rs:744) O2后verify与emit之间；[build.rs:1634](/workspace/zil-push-main/crates/zil-backend-llvm/src/build.rs:1634) 现有object选择/发布；[machine_object_patch.rs:718](/workspace/zil-push-main/crates/zil-backend-llvm/src/machine_object_patch.rs:718) 可借其完整base摘要、symbol inventory、事务和失败回退，但其derive实际做objcopy改名/localize＋ld.bfd -r，**不保证去除旧body、原顺序/节表/符号表或最终ELF逐字节相同**。需要新的byte-layout实现，不能套一个函数名即声称完成。

最小完整链路：

1. 完整原Source/当前独立V/默认预算/完整CGU Builder与LLVM preflight→每CGU完整 `default<O2>`→verify。读**实际优化后IR**，不用诊断program.ll或原LIR冒充优化结果。记录原function顺序、linkage/ABI/attrs、target/DL/CPU/features、machine opt/reloc/code model、LLVM build identity及相关module flags。
2. 首个完整CGU照常发射，建立此次请求内可信producer片段。候选仅固定target下的primitive叶：没有非intrinsic调用、内存/地址逃逸/常量池/jump table/blockaddress、inline asm、personality/GC/COMDAT、debug/stackmap/patchable/instrumentation等；不能只检查返回类型与“没有global relocation”。允许的intrinsic逐项说明机器降低是否会引用外部符号；trap也不能笼统视作call-free证明。
3. 后续CGU先完整O2，精确比较canonicalized optimized body＋完整codegen envelope；只忽略经证明无语义/机器上下文影响的名称编号，hash只是索引，必须完整比较。跨CGU的module flags、函数alignment、section、calling convention、attrs、安全/栈保护等不许丢。使用 `available_externally` 仅在O2完成后抑制重复body机器生成，不合并函数身份、不改调用目标。不足以证明的函数正常发射。
4. 从首producer提取symbol范围代码，独立解析确认无向外fixup；对重复函数恢复各自原external symbol和独立地址。按**原CGU函数顺序**重建完整`.text`、alignment/NOP padding、st_value/st_size及相关relocation/section/string/symbol tables。非复用函数也因缺失叶而改变临时offset，必须一并映射。原relocation指向section symbol＋addend时，addend也要按原目标位置恢复，不能只移动r_offset。
5. 完整验证重建object库存/符号唯一性/边界/relocations后，才按当前请求发布object/link缓存。key区分策略版本、当前optimized input/envelope、producer身份与布局，不继承旧unit授权。取消、重建拒绝或捕获不完整时丢弃派生物，恢复**原完整输入的fresh native路径**；不能在已经机器emit变异的LLVM module上简单改回linkage后继续当fresh。

隐藏风险与首版收口：

- `.eh_frame`/CFI/FDE是函数体外依赖；还包括LSDA、debug、stack maps、地址相关notes。首版可拒绝带未实现相关节的整个CGU，unit1证明这不是必然零覆盖；后续按FDE关联复原，而不是盲目复制producer CFI。
- 相同IR不是不附条件的相同machine bytes保证：机器层函数编号、局部label、alignment/max-skip、符号/section顺序、branch relaxation、module flags/ABI属性均可能参与输出。无跨函数PC-relative操作的闭叶较易证明；survivor的跨函数relocation与整体布局仍需处理。不能复制source函数尾部padding充当任意目标offset的padding。
- 不建议用单独小module编首producer；那会更改module context/布局并另付流水线。先完整CGU供种，后15个unit复用，是更小且容易审计的第一批。
- 既然本任务不重建LLVM，不以新增SDK hook为前提。现有MIR/cohort代码的closed-domain拒绝项可参考，但其WorldBound证明/opaque typed snapshot不自动授权ELF片段复用。

可证伪验收：新单独窗口中，对每个派生unit与未改路径实际完整object做逐字节比较，再执行原完整ELF门；应保留2504定义、每个原符号地址/长度及16-unit ownership。收据必须新列完整O2次数、producer实际执行数、被省略的机器函数数、key/重建/fallback ns、当前原验证次数。若只是body一样但ELF不同，本路线的“透明复用”目标未闭合，保持失败；不借质量例外放行。覆盖cold/broad/private/error/recovery/restart、ABI/attr/alignment/reloc/CFI突变及伪造缓存/取消反例。先验证机制净收益，随后才扩域；不能承诺2300叶能省掉全部1.35s，100个聚合value本身仍是大机器体。

## 路线2：纯exe合同下，完整O2后移除不再有用户的机器体

此路线直接消除不需要的emit工作，允许产物变化，应与路线1分开验收。复用现有 [late_closed.rs:13](/workspace/zil-push-main/crates/zil-backend-llvm/src/late_closed.rs:13)、[build.rs:670](/workspace/zil-push-main/crates/zil-backend-llvm/src/build.rs:670)、[native.rs:2315](/workspace/zil-push-main/crates/zil-llvm-api/src/native.rs:2315)。已有`--late-closed-executable`、plan binding、post-O2 use-empty/internalize/globaldce和独立receipt；这不是待发明的LLVM pass，也无需LLVM重建。

实施应补齐**真实产物导出合同→根集合→缓存身份→独立质量验收**，而不是将所有源码private直接internal。可执行文件并不天然没有FFI导出、外部链接对象引用、动态查找/注册或callback地址观察。仅明确无外部ZIL-export合同的纯exe适用；保留main/foreign/runtime、外部对象需要的定义、跨CGU引用、所有地址用户与未知root。源码pub也不等于必须从native exe导出；反之源码private不等于地址绝不逃逸。根来源不完整时拒绝优化。

当前late-closed已经排除foreign/environment、预O2跨CGU引用和非直接call地址使用，绑定实际machine输入；post-O2只选use-empty且无特殊ABI/metadata/COMDAT的external定义，随后globaldce。这是第一批最合适边界：**首轮完整原O2仍优化全部输入，只省后续孤立helper机器发射**，不改变之前的inliner决策。本窗大量helper很可能已被inline后无用户，实际可删数量须由新post-O2 receipt证明。若剩余用户较多则否证收益假设，不扩大删除条件凑数。

独立完整质量闭环（不是修改旧fullELF门）：

1. 新候选/新策略/新窗口，与旧正式窗并存；原1000 research及stock fullELF失败、comparisons空永不改写。字节变化记录为变化，不直接推出wrong-code，也不自动Pass。
2. 候选相对同源fullO2控制，先核**post-O2、DCE前**完整函数IR/attrs相同；给每个removed函数可复核的pre-O2 root排除与post-O2零使用证据。保留定义IR/ABI应相同；任何额外改变必须单列解释。symbol/relocation closure证明不出现悬空调用、外部引用丢失、callback身份改变。
3. 解析完整ELF所有sections，列清删去body/符号、布局/relocation/eh_frame移动与任何其余差异；对保留函数作符号解析后的反汇编/CFG与指令语义对照，不将任意`.text`变化笼统当metadata。对已有41/42B叶和简单聚合函数可做独立位向量/路径等价检查；涉及未知指令或UB/poison边界不能用少量oracle代替证明。
4. 全原输入通过currentV/default budgets，既有native oracle全跑；补i32边界、checked overflow/trap、负余数与失败清理、跨CGU调用、address/FFI/动态root反例。运行质量另比较动态指令/分支或同条件runtime，代码体积、对象/磁盘增长、冷/广改/私改编译总时均记录。结果只能作为事先具名独立quality合同的评审材料，不能追溯覆盖正式exact-ELF结论。

跨CGU薄body import建议：第二阶段再做。当前 [layout.rs:87](/workspace/zil-push-main/crates/zil-backend-llvm/src/machine_cgu/layout.rs:87) 的Complete importer会生成`zil.import.*` internal实体并改direct call，未内联的副本仍要emit；本窗默认关闭，不能说它已经重复付费。新模式应保留唯一canonical owner和真实地址，importer仅用`available_externally`优化body、未内联call仍指canonical symbol，不发射import副本；先证明纯exe下同名定义不可interpose。按实际direct-call需求与ABI/global依赖导入，绑定callee body及传递优化依赖到对象key，变化时使所有import消费者失效；未知本地状态/地址身份保持原路。fullO2对每个实际CGU保持完整。它可能增加IR O2、inline code大小并恶化private-edit扇出；本窗main→value是否受益尚无证据，优先级低于已测emit重复域，不能宣称能补齐Rust差距。

交付边界：路线1具备明确可实施入口和可证伪目标，但完整ELF重建尚未实现/验证；路线2机制已有，需闭合产物根合同及新质量验收。当前证据支持开工，不支持宣称已有净加速、全面胜Rust或接受旧负例。
