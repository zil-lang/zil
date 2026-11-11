# ThinLTO complete-visibility：Python与正式协议非作者独审

结论：**v2静态审查无新增阻断，允许进入既定真实执行；wrapper整链尚待实际验证。** 本审未build、prepare、执行编译器/native、运行LLVM工具或新增性能采样。root已启动唯一质量窗口，本文不提前报告任何执行成功、覆盖证明成功或运行门结果。

## 固定版本与首个静态阻断

新目录为`research/thinlto-complete-visibility/`，未修改旧default目录。source固定`2549cbec8e1f3096d4c8c8a960459c83095ad13d`、tree `ebb993128355fed8228627c6d7b37d5cb0a90d45`；B binary冻结SHA `fdfe6cea83286475582ce5b9063e9d84a9371b584baf99920193689f5e93d3e4`。source侧原planner/预算与合法输入证明由另一个非作者source-review负责，本文件只审Python如何消费并核对真实LLVM证据。

v1 prefreeze `9d588d0ce1c89c49448a4b171f2502345095414e4a539504275688f3334645a4`被root和本审静读发现同一确定错误：finally把`originals`从按unit的parsed IR字典覆盖成临时文件Path列表，后置check_modules必拒绝。v1完整保存在ROOT外`research/thinlto-complete-visibility-history/v1/`；没有用一次失败编译掩盖这个静态错误。

v2 prefreeze SHA `ebe4fc9d30ecf77b261d846497b7c863260e21b94f655c6d0d748f4cac6e262f`。全部声明文件SHA实算匹配、Python AST通过。wrapper SHA `aed889bb0c519e8690cc663e61b32f1125ed648dd145e992a8e81172b7449792`，对v1逐字节确认只把finally的临时列表名改为`saved_original_paths`的两处引用；其他算法/门/LLVM参数没有随修复变化。

root明确取消额外mock作为阻塞门，作者未创建/运行mock。本审照实记**未运行wrapper mock控制**；旧parser自检不能算wrapper整链通过。下一次真实编译是首轮集成验证，任何proof/工具/文本不支持都必须在native运行前停止并保留Incomplete，不因本静态审结论而豁免。

## 唯一LLVM参数变化与当前证据绑定

逐读新wrapper与旧default v4：actual per-unit clang仍为原IR/O2加-flto=thin/-march=x86-64/-fPIC；link的O2/CGO2、PIC、CPU、ICF none、after merge且aliases false、两jobs/两threads、save-temps及cache策略保持。唯一新增的LLVM选择参数是一次`-Wl,--mllvm=-force-import-all=true`，未调整size/hotness数字。

该参数只放宽LLVM的大小/noinline导入选择，不保证合法性或完整覆盖；官方LLVM23 FunctionImport.cpp还可能记录无法import的诊断后停止某caller扫描。因此Python不能以flag为证，而必须通过前置/后置集合检查。

每个unit先从actual input.ll的定义集合匹配Rust proof的唯一canonical owner unit，不依赖调度vec索引；保存input、prelink BC、实际dis输出和identity。link前要求8个唯一proof unit、同一当前machine receipt SHA、保存文件身份未变，实际link输入的路径和字节SHA与这些BC吻合，不能用另一个候选/旧默认的见证代替本次当前请求。

## Required、Allowed、Actual及预算检查

`visibility.py`的检查顺序与边界如下：

1. `export_proof`要求原complete planner、canonical owner相等、原Required恰等Allowed三项当前证明；8个unit按0…7唯一编号，canonical全局不重叠，Allowed有序去重、只能跨owner且不能包含main。每个导入cost为正整数且同名一致；每unit与全局copy_cost重算相等，总copy_cost≤原sourcecost。
2. actual input.ll必须完整匹配各unit canonical集合；按block+instruction复算全部sourcecost（只排唯一main wrapper）和所有Allowed函数cost，与Rust proof逐项匹配。不能仅接受布尔“预算通过”，也没有提高预算以容纳强制导入。
3. 从八个**actual prelink**模块的每个当前定义出发求直调闭包，建立跨owner Required；只跳过精确已知的foreign或有限LLVM23 intrinsic。Required必须属于原Allowed。这一阶段定义正确区分“helper已在本地prelink内联进callee”与“缺少callee可见性”，不强迫原machine闭包每个helper符号再单独import。
4. link后对每个actual `.3.import.bc`按其原unit输出路径唯一关联，真实verify与dis，再要求Required⊆Actual definitions。所有AE必须在Allowed；所有其他definitions也只能是own canonical或Allowed，不能换成非AE linkage绕过额外导入检查；实际post-import调用/声明也受owner和精确foreign范围限制。
5. 每份`.4.opt.bc`继续真实verify。必须同时有8份actual imports文件和8份postopt模块；presence of declaration不能替代body definition。

这里的Actual包含owned与imported定义，实质导入集合为Actual减own；因Required为跨owner，Required⊆Actual及Actual⊆own∪Allowed正好表达所需两侧约束。Allowed全体的原成本已在预算内，实际imported集合是其子集；不把LLVM优化后指令数冒充原machine复制预算。

reader不是一般LLVM解释器，只覆盖当前pure一行instruction子集：未知/quoted函数名、alias/global/地址引用、间接call、invoke/callbr、未知新定义或不支持结构均失败；有限intrinsic名单没有`llvm.*`通配。它从全部当前owned定义求闭包会保守过估dead根，可能拒绝一个合法但不在支持域的产物；这种结果应记witness/coverage不成立，不能偷删Required或称runtime NoGo。

link stderr完整保留，只有精确Failed-to-import形状且目标为已证明无owner/body的foreign或实际prelink已声明的有限known intrinsic才允许；未知消息/源函数失败拒绝。即使接受无body诊断，后置Required覆盖仍必须成立，故不靠“诊断可忽略”放过扫描中断产生的漏导入。

## 固定计量、运行门及失败处理

processes.py与已审default v4逐字节相同：外层拥有ZIL私有session，wrapper actual工具继承该group并核direct-parent/session身份；异常先落partial后杀私有group，outer清理leader退出后的残留。工具identity保留ld.lld argv0，版本/工具字节、source/tree/binary/buildlog仍绑定；不能复用旧eb5的binding冒认2549。

仍为n64/base、n256/base，原动态pure函数、8函数/leaf、100000轮、周期251/模1000003、三输入及六排列不变；6次clean编译、54native（36formal）。全部formal均值且全部I/O有效，每case必须B/A≤1.10和R/B≥.90；无缺格Pass、无删长尾或用Rust门救相对退化。任何proof/工具错误在wrapper返回前失败，run保存raw与Incomplete；真正完成的运行质量失败才是NoGo。

新增原planner clone/proof、IR解析、实际dis/verify与工具hash成本都在完整请求wall内，不减掉，也不据这份wrapper cold wall声称编译净提速。范围只限两pure base，不能替代原八snapshot、函数地址/global/resource接口或产品采用门。

## 旧reader自检的真实含义

已有`parser-existing-default-selfcheck`读取旧16份prelink与32份decoded .3/.4，记录N64 unit0 Required56、N256 Required224，其余0，并拒绝旧空import .3。这与本审对旧raw的独立观察一致：leaf value已经内联本地helpers。该自检的Allowed是从旧文本合成，仅证明reader在这些保存文本上能求Required并拒绝缺body；**不是新2549 Rust proof能力收据、不是mock整链，更不是新候选质量结果**。

## 正式protocol追加核验

实际protocol为`out/next-optimization/thin-complete-protocol-v1/protocol.json`，SHA `5b0ff104485ba18c34db170716ead50cc438bfc3b8c8ebc1c1d1266876946728`。其source是2549而非旧eb5，B binary SHA为上列fdfe；build binding元数据source/tree/binary/release成功相符。与旧default protocol逐项比较，完整case manifests（含sourcehash/独立oracle）、iterations、orders、limits、counts、cpus、jobs和scope完全相等；A/R及LLVM全部工具的路径与冻结SHA也相等，仅B exporter身份按新source更新。

质量窗口开始后遵守root限制，没有重复大文件hash、调用LLVM工具或新增任何执行；本审在窗口前已实算prefreeze全部小脚本hash及protocol SHA，窗口内只做小JSON/两行rename比对。正式runner仍按冻结逻辑核实际工具/源文件字节。当前结论是**静态允许执行、真实整链和实际coverage/运行门待本次结果**，不是提前Pass。
