# Native prelink 测量器非作者静审

**Pass：可 prepare 并执行冻结的两个case完整请求 screen。** 本结论审的是 Python 测量器；我同时是 native API 作者，因此它不代替另一个审核者对 Rust API 的独立源码审查，不代表实际编译/运行质量或净编译收益已通过。本次没有 prepare/import 测量器，没有运行编译器、LLVM、native 程序或读取大工具/DSO做hash。

绑定 `research/manual-prelink-screen-freeze-v1.json` SHA256 **9d959c5d5017c7f73e76a76c92db966ba18a6a7a98f62454979ce41ca690ed1d**；候选精确 source **2321e74897975978925dc7d2a611f7d672f716e1** / tree **03fa3d83dc5ba80d7849f7148b97b7d12449700b**，成功构建所绑定 binary SHA **ce90681f261c109c412bb9d513e3c0ff58680a2e593d798cdf21b3e27daa407a**。独立重读10个文件哈希与freeze一致、9个Python AST通过；processes及vendor5文件逐字节等于原linear版本。完整hash在同名JSON报告中。

## 真实字段与完整输入门

当前 Verify 必须一次实际执行成功，source closure数量精确；B需要实际原machine、partition输入、8unit当前预检，当前typed Complete owner唯一、原Required==Allowed、各函数/单unit/全局费用及原sourcecost界。首object report须与新外部preflight收据逐对象相等，后面恰好8个真实objects；不允许fallback或cache。

八份 prelink receipt 需包含同次 actual definitions/declarations/direct_calls/intrinsics、canonical、当前结构化预检、before/after LLVM验证和完整目标/管线/计时。检测全图定义唯一、调用边有真实定义或声明、intrinsic来自实际非零IntrinsicID名单。Python从全 canonical prelink图独立重算transitive Required，核其落在原Allowed内，再与各postlink收据required/allowed/imports/donor_modules精确比较。没有以旧原machine闭包强迫已经被prelink内联消失的helper继续独立导入。

所有实际imports必须有固定私有名和正direct_uses，八个postlink objects必须为真实manual-prelink+private+thinlto<O2>+after/aliasesfalse管线、x86-64/PIC/default、当前实际prelinked-bitcode输入、完整LLVM验证和emit证据。新路径没有伪造仍在运行旧plugin/summary/index/外部opt/dis或八hook。集合型direct_calls不能反推精确Use重复计数；逐Use角色正确性由冻结Rust实现、独立源码审核及真实合法地址反例控制承担，外部收据只作结果，不是传回Rust的许可。

最后命令只能是一次固定Clang普通-O2 target链接，正好8对象，不允许LTO/plugin/ThinLTO参数。大工具/source/build/DSO/loader环境与原reference在screen前后完整对称校验；各arm所选compiler每请求前后全文SHA也在计时外，B内保留实际所有compiler校验与算法成本。Clang/native LLVM23.1.4和Rust1.99.1 bundledLLVM23.1.1明确区分。

## 数学、调度和质量判据

整个cases对象必须等于原linear协议 SHA `a30ffe86586c77849a613be636ece4f58fa3bfcebbc0eec3191cba333ce4e72c`：N64/N256、八函数/文件、无编辑base、100000循环、相同源字节和oracle。人工复核源生成公式与独立整数evaluator分开，重新用原251个seed值验证周期总和与全部6个输出向量：`100000=398*251+102`，重复周期加余数再模1000003与逐轮acc reduction相等。N64/N256最大acc+value分别1126005/1510005，max(input+i)=100254，均在i32范围；中间单函数coefficient*y+constant同样受小整数上界约束。

六排列 ABR/BRA/RAB/ARB/RBA/BAR 每arm在每个位置恰好2次。两个case各6轮×3arm，合计36个完整冷进程编译、108次真实native IO，其中输入41为36个formal，其余00/ff为72个functional。每arm每case必须保留6个formal和6个compile样本，没有删样；报告完整wall/CPU/RSS及长尾。每case都要求输出正确、runtime B/A≤1.10且Rust/B≥.90，编译wall/CPU是否均降低另列，不拿runtimePass替代compilePass，不以假想分布总分覆盖单case退化。

同arm六次ELF必须唯一，A/B各自material IR跨轮一致；material IR只排唯一已知verified-fingerprint行，其余字节保留，不强迫跨arm同码。完整编译启动至wait4均在计时内；不是从旧Python研究耗时中算术扣诊断成本。RSS保留wait4口径及launcher/fork高水位floor限制，不能称aggregate peak。

## 失败保全与首次接口差异

compile run先写raw再检查compiler身份、report或候选收据；native先写raw再解析stdout/stderr并保存每次有效/无效记录。异常有process-attempt和fatal/Incomplete；固定计数不足或同源产物波动同样失败且不删除raw。输出错误不会被筛掉；完整矩阵完成但运行质量失败可报告Complete+NoGo，不能将缺字段、图/预算/Verify失败伪装成性能NoGo。

静审首次发现新原始buildbinding的binary是 `{path,sha256}`，旧validator要求扁平字段。root新增正常化 `binding-manual-prelink-2321.json`，SHA **1f6fd80b428a2d6f256691b532db1866e9275e25925f53fd57628327ad12c602**，保留原binding不变且validator没有降级。已独立核对其exact source/tree/binary声明、release成功raw SHA **d2e563605cafb803d38a8b099363e32b32234697f88842c754458d18acdb1af4**、完整原binding SHA **33c3634cf52ae203eb30e9594e8d9ed0179f3d66a24e5fcba1ca7b45f8ddf695**。旧linear result format残留与README施工状态也在最终freeze前修正。

一个保守边界仍明确记录：若原合法intrinsic在全部实际prelink输出中都被优化删除，当前foreign集合相容门可能使请求Incomplete；这是更严的接受集限制，不是性能NoGo，不允许悄悄用llvm.*前缀放行。当前两个固定pure case之外，不外推地址/global、暖缓存、多点、并发或生产采纳结论。

## 已准备的实际协议追加核验

**Pass**：实际 `out/next-optimization/manual-prelink-protocol-v1/protocol.json` SHA **a2999b0eb482994de4516f8de6760ca6dd4a7b99fe722c8b0d75aaa9daa76810**。完整cases对象与原a30协议相同；实际冻结source files逐个全文hash匹配，所有9脚本也与freeze一致。已核CPU `[0,1]`、jobs2、六排列、36/108/36/72固定计数、每case1.10/.90门、两个buildbinding的文件SHA/source/tree/binary路径与SHA声明、真实release成功日志SHA；producer明确为2321/tree03fa/ce90681，不误绑后续仅脚本的Git子提交。

DSO协议绑定 `toolchain/llvm-23.1.4/lib/libLLVM.so.23.1` 调用路径与resolved路径、SHA **2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250**，环境精确为该lib目录的LD_LIBRARY_PATH；Rustbundled23.1.1区分字段正确。本追加只核小文件、源字节与身份声明相容性，没有重新hash大binary/DSO；prepare与实际screen仍须按冻结validator执行相应全量身份检查。没有由本审核者启动任何采样。
