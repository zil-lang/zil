# 严格原预算下的 ThinLTO 完整可见性机制对照

状态：Python待非作者独审，未prepare正式协议、未运行新候选的N64/N256编译/native；只读解析旧default数据自检已完成。源代码固定远端2549cbec8e1f3096d4c8c8a960459c83095ad13d，tree ebb993128355fed8228627c6d7b37d5cb0a90d45，parent eb5。author499798同tree；root已完成exactremote release、5个proof测试与无native check。完整绑定由out/next-optimization/binding-thin-complete-2549.json提供。默认ThinLTO NoGo的源码、脚本、协议和raw均保留原目录，不修改、不叠加旧sink/position候选。

## 单一机制变化

前一个default ThinLTO在该fixture的8个imports文件均为空，两个runtime case失败。本对照只在实际lld参数上增加一次官方LLVM23 `-Wl,--mllvm=-force-import-all=true`，不调size/hotness数字。CPU/PIC/default model、O2 prelink、ThinLTO O2/postmerge/aliases=false/ICFnone、两CPU/两jobs、LLVM verifier、Rust基线、输入及运行门沿用原方案。

force标志会跳过size/noinline选择过滤，但不保证所有合法目标都导入；遇无summary目标甚至可能只写错误并break当前caller的扫描，不是fatal返回。因此标志不承担预算或覆盖证明，实际.3模块不符合证明时不进入runtime。此对照没有产品采用、编译提速或功能全集覆盖结论。

## Rust exporter：沿用原planner和原复制预算

仅研究build.rs、machine_cgu一行module接线及complete_visibility研究模块；公共schema/Cargo不变。现有checked exporter守卫全部保留，还必须 `ZIL_RESEARCH_THINLTO_COMPLETE=1`。Clang/O2/incremental-o2/after/LinuxMain/8units2jobs，无closed/objectcache；保留当前O2前端fold、独立Verify、原model完整preflight、fold后分区输入及实际unit完整preflight。语言或复制预算没有提高。

fold后、machine消耗前，clone调用原ResourceHelpers/Complete planner，代码与其成本限额不动。当前pure范围拒closure/global定义或声明、local definition、间接调用、函数/global地址操作数、未知声明或目标；callee仅精确Val::Global可当直调，call参数及其它操作数独立递归检查，不能用references减direct_calls抵消地址使用。只支持原source_name的完整名字（含.lc1）、main wrapper和受限外部声明。

proof从实际原计划中的private zil.import定义反查source、核linkage与原function.cost。Disabled canonical分区与原计划按每unit canonical定义集合建立唯一双射，不按vec索引假定相同。原machine直调跨owner闭包必须与原实际复制名单完全一致，总copy cost按unit累加且≤原LIR source函数总cost（排wrapper/helper/decl）。因此Complete超预算后静默退ordinary8unit/fallback清空仍会因缺Required失败。任何scope不支持或证明缺格都显式错误，不降级为“已证完整”。

源码5控制已由root实际跑过；非作者source-review.md记录有限边界：nested-address控制本身不是合法typed最近反例；当前完整preflight挡住Val::Invalid，但no_address的wildcard对未来enum扩展缺少编译提醒。不能宣称这些未来/合法地址反例已经完整覆盖。该限制不放宽本次pure固定输入。

## Python：从实际prelink到实际.3的证明

visibility.py是保守、失败关闭的pure LLVM文本读者；不解释一般LLVM语义，不删除后缀猜canonical名。每unit：

1. 在原input.ll上匹配Rust证明的canonical集合；同一八unit必须完整且唯一。保存输入、prelink BC、实际llvm-dis输出及SHA；actual linker inputs必须等于这些BC字节。原machine收据SHA贯穿全部unit和link。
2. 用actual input.ll的block数+instruction数复算sourcecost及每个allowed函数cost，核对当前Rust证明。source排除唯一main wrapper。此reader只支持原renderer一行一条instruction的pure子集，不能拿一般LLVM复杂多行结构猜成本。
3. 在八个actual prelink定义构成的直调图上，从各unit所有当前定义出发（保守包含dead定义），计算跨owner Required。只能跳过无owner/body的原exact外部声明及有限、明确的LLVM23已知intrinsic名单；完整实际opt verifier另行执行。未知函数/alias、间接调用、地址值、quoted名字、LLVM新未知clone均失败。不存在llvm.*泛放行。
4. 要求Required⊆原Allowed；原machine中已经被本地prelink内联掉的helper不强制要求另import。Required来自当前prelink图，不是原machine符号闭包。
5. 链接后匹配每个actual `.3.import.bc`，执行opt verify并llvm-dis，要求Required⊆实际.3全部definitions；AE definitions⊆Allowed；所有其它definitions也必须属于own canonical或Allowed，不能以非AE linkage逃过检查。实际post-import引用也受同一名字/owner边界限制。只导入声明不满足覆盖。
6. 每个.4.opt.bc仍实际opt verify。保存实际imports文件、全部可取得LTO临时BC、过程命令/输出/时间及native cache前后列表。save-temps或link失败先保已有raw；硬SIGKILL可能来不及复制全部partial，外层Incomplete及已落盘raw如实保留。

force错误stderr完整保存。只允许精确“Failed to import function SYMBOL due to REASON”形状、且SYMBOL为已经证明无owner/body的原foreign或实际prelink声明中的有限已知LLVM intrinsic；未知消息/源函数失败一律拒绝。接受外部无body消息不代表扫描成功，后面的Required实际覆盖仍必须成立，故break造成漏合法调用会被抓住。

## 旧真实数据的只读reader自检

parser-existing-default-selfcheck/result.json读取旧default-v1实际16份prelink.ll和32份decoded .3/.4；不启动编译器/native、不记录性能。N64 unit0 Required=56、N256=224，其余unit=0；旧无import的.3明确缺这些定义而拒绝。LLVM已生成的有限vector.reduce.add intrinsic可按精确名单识别，其余未知名字仍拒绝。

**该自检Allowed是从旧actual input文本合成的原闭包/成本，只测试reader与拒绝旧空imports；不是2549的实际Rust planner能力收据。** 正式请求只认本次新Rust proof，不用这个synthetic对象代替。保守dead-root过估或不支持文本形状属于witness/coverage失败，不冒称runtime NoGo，不修改预算补过。

## 固定协议、计量与执行

prepare.py参数同原两case脚本：out、baseline、exporter、rustc真实toolchain路径、clang、ld.lld保留argv0角色、opt、bcanalyzer、dis、两份build-binding、cpus（两核）。source/tree/binary/buildlog全部绑定，工具路径+字节SHA固定；vendor5原字节、动态生成器和独立周期oracle保持。prepare不编译；先非作者审脚本，再root prepare，审生成protocol精确SHA后 `run.py --protocol ABS --protocol-sha256 SHA --out NEW_ABS --execute`。

固定n64/base、n256/base，8函数/leaf、100000轮、251周期、模1000003；6clean编译、54native（36formal），输入00/41/ff与原六排列ABR/BRA/RAB/ARB/RBA/BAR不变。每case完整6formal算术均值，全部I/O正确且B/A≤1.10、R/B≥.90才Pass。缺格/异常不删样本，任何proof失败停止该次运行并留Incomplete；任一实际runtime失败保NoGo，不因快于Rust而豁免相对退化。

沿用已验证processes.py：外层私有session，wrapper工具继承ZILgroup并验证parent/session关系；内层deadline先落partial raw再杀私有group，外层组外wait4并清理leader退出后的残留。正常及内/外timeout控制在原目录已有记录，新目录不重跑。所有编译wall都含额外clone/planning/proof、反复工具SHA、IR解析与验证，只有runtime质量用途，不能据其编译wall作优化宣称；RSS为wait4口径，非aggregate peak且可能有launcher/fork floor。
