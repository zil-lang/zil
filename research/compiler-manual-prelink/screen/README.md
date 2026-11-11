# Native C API prelink：独立测量器草案

新目录，未prepare、未生成数据、未freeze、未编译或执行native。common/prepare/compile_screen为当前施工文件；processes.py与vendor五文件保持linear原字节。候选源码已固定2321e74897975978925dc7d2a611f7d672f716e1/tree03fa3d83dc5ba80d7849f7148b97b7d12449700b，release构建已由Root完成；正式flat绑定out/next-optimization/binding-manual-prelink-2321.json，binary SHA ce90681f261c109c412bb9d513e3c0ff58680a2e593d798cdf21b3e27daa407a。API receipt按实际Rust字段接入，测量器代码现冻结供prepare，实际协议及运行仍须核验。

## 固定对照

原linear协议a30ffe86586c77849a613be636ece4f58fa3bfcebbc0eec3191cba333ce4e72c的整个cases对象相同：N64/N256纯base、八函数/文件、100000循环、所有源字节/251seed oracle/00-ff-41 vectors。六排列ABR/BRA/RAB/ARB/RBA/BAR，每case六轮×A/B/R：36完整编译、108native=72功能+36formal。B/A≤1.10、Rust/B≥.90不变，完整wall/CPU/RSS和所有长尾保存；无删样或复测替换。

A仍main990、R实际Rust1.99.1；B为待pin新native compiler，不冒认旧exporter2549+插件。两个ZIL命令同为build --backend llvm-api --opt O2 --incremental-o2 --function-merging after --codegen-units8 --codegen-jobs2 --clang pinclang，无objectcache。只B设置独占新绝对ZIL_RESEARCH_PRELINK_RECEIPT，每arm前及native前清除旧ThinLTO五项、private四项和新receipt环境。没有clang wrapper、外部opt/dis/plugin、summary/index；最终普通pinclang链接。

A/B/R所选compiler每request前后在计时外对称核字节，所有固定工具/source/buildbinding/scripts/reference在screen前后完整预核；完整actual compiler启动至wait4计时内保留全部语言/机器/API/LLVM验证。没有旧driver十image首1GB B内哈希；不把移出测量的验证冒称native算法收益。两条pipeline自身实现/调用边界不同，完整结果要另测，不能从旧session相减。

## 实际准入

runner核current Verify一次、完整source closure、A原标准LLVM23.1.4 API O2收据。B必须实际收到 `zil-manual-prelink-preflight/1` 原模型/分区/八unit当前预检，首object_reports与外部receipt完全相同，后续exact8objects，无fallback/cache，summary_index=false。

proof保持 `zil-thinlto-complete-visibility/1` typed Complete 的current flags、唯一owner、原Required=Allowed、每函数/每unit/总费用与原界。八份prelink `zil-research-manual-prelink-unit/1` 必须有真实definitions/declarations/direct_calls/intrinsics、canonical、LLVM/target/PIC、ordinary-no-summary、实际before/after verify和当前structured preflight；事实集合和图必须与owner相容。intrinsics来自当前LLVMGetIntrinsicID精确名单，不按llvm.*泛放行。

harness从八份actual prelink facts独立重算transitive Required，要求Required⊆原Allowed、外部声明/直调仅在canonical/精确foreign/实际intrinsic集合。八份post `zil-research-manual-prelink-object/1` 的required/allowed/imports/donor_modules必须精确相等，private名固定且direct_uses为正；actual_bitcode_graph_checked及verify_parse_trim_link_private_postlink必须true。标准对象须完整manual-prelink/私有导入/thinlto<O2>/after aliasesfalse的实际pipeline、current-prelinked-bitcode输入、LLVM23.1.4/x86-64/PIC/emit证据。

新路线的逐Use与actual module校验在原生API内执行，外部JSON只是结果收据，不能成为传入Rust的授权能力。graph的direct_calls是目标集合，不能独立还原重复调用的精确Use数量；harness验证实际coverage、私有名及正计数，逐Use正确性仍依赖冻结源码独审及实际原生验证。没有假称仍运行旧external opt/dis/全文差异或plugin八hook。

最终report.commands必须只有一次pinclang普通-O2 target链接、exact8objects，不允许flto/plugin/thinlto参数。每case各arm六轮ELF必须唯一，A/B各自materialIR跨轮相同；不要求跨arm同码，实际质量用原oracle及运行门。缺/未知字段、身份变化、工具失败、Verify/graph/预算失败均在native前落Incomplete及已得raw，不能把遗漏算Pass。

## 未执行

只做AST语法读取；没有import/prepare或代码生成数据。prepare需真实A/B成功build receipts、candidate exact commit/tree、原linear protocol及实际tools；默认不能以占位binary/receipt运行。本次冻结的是测量器代码及正式源码/构建绑定，尚未prepare或运行；生成协议后还须核其实际工具/source/case身份。两个纯base通过也不代表原八case、地址/global、多点/并发、总体权重或生产采纳。

## LLVM DSO与实现身份

prepare必须显式--llvm-dso，固定ZIL A/B共享的实际LLVM DSO调用路径、resolve后真实路径与SHA `2bf890dac0d0a9e2bf2a81abf0a611e6893a06cbd827c525abaa1114f1ec8250`，单独于四tool集合。protocol绑定当前LD_LIBRARY_PATH（区分unset与空值）；screen开始/结束validate对称核实际DSO全文SHA/解析路径及loader环境，均在所有请求计时外，不在每次B内重新读大库。B内所有实际API校验仍保留。ZIL/native/Clang实际LLVM23.1.4；Rust1.99.1的bundledLLVM为23.1.1，rustc-vV需核这两个实际字段，不把它说成同LLVM patch实现。本次仅写字段，未读取或hash大DSO。
