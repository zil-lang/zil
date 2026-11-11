# 完整 O2 producer 测量器非作者静审

**Pass，可 prepare 冻结协议。** 本次只审 Python 测量器及小构建收据；没有 prepare/import、编译、LLVM/native 执行或读取大 binary/DSO 做hash。我是 native API 作者，本报告不能充作 API 的独立源码审查；该部分另有 `manual-prelink-full-source-review.md`。

固定 source freeze SHA **684a858a2a7d4c58d29395b87c3d2a5dda0bd808588e25074074fb8eae001a31**；producer **2458defcaf8be51c87b80c0dadf391bb585735e8** / tree **e917926bc8976da90e58897911c72978b9a24f88**；成功构建绑定 binary SHA **30140135c39706e8251bd5da3eb9acb4e8481a9c12acbc4aed6403f57a768097**。实际重读10文件哈希、9Python AST通过，processes和vendor5文件与a299原字节一致。

三份Python源码 SHA：common **cb72623ca3e22dead7ddda13b711a2e7625100d6e82510f2d7c3446dd1bc637c**、prepare **76cfe5eebc0af9b527116a62104fc295c7ae1b3a5d9e91f9cd0df11783786c39**、driver **5729a4c997c5cdbf6011f8653bcecf012b1b36048110596f49c34c0f89017e94**；最终README **fdf8cad9745baba31e8c296dfd0bfa80e4120c78a5da8e60594da8f5933ab5ae**。完整字段在同名JSON。

## 新分支检查

与已审a299测量器的逐字节diff，只涉及新协议/result标识、unit/2和object/2、producer/default<O2>、新pipeline与分支精确门，未改变源、oracle、计时、调度、门限或失败保全。

八个producer必须实际回报 `zil-research-manual-prelink-unit/2`、完整 `default<O2>`、正优化时间、当前structured preflight、before/after LLVM验证、canonical/definitions/declarations/direct_calls及真实IntrinsicID名单。外部测量器继续从八份实际图独立重算transitive Required，不以原Allowed是否为空决定分支；原Complete owner/原Required==Allowed/原function.cost费用及总界不变，未知符号、缺body、额外imports一律失败。

object/2必须为新pipeline：

`canonical-full<O2>|budgeted-direct-imports=private|empty-required=emit|nonempty-required=thinlto<O2>|merge-functions=after|aliases=false`

actual Required为空时强制布尔 `postlink_optimization_ran=false`，`Object.optimize_ns`、`research.postlink_optimize_ns`、`trim_link_ns`均为整数0，donor_modules=0且imports=[]；Allowed可以非空，但不产生第二次优化许可或工作量。Required非空时强制布尔true，两个优化时间必须为正且相等，trim/link时间为正，原全部精确imports/private-name/positive-use/Required/Allowed/owner检查继续执行。

`actual_required_path_verified=true`替代旧的全步骤执行布尔，空路径没有伪称trim/link/private/O2已执行。测量器分别报告8个实际对象和实际第二轮优化unit数，避免以“8objects”冒充“8次postlink O2”。当前full producer、真实parse/sealed/LLVM检查来自冻结受检原生实现；外部JSON是结果，未成为Rust授权输入。

## 保持的对照与失败合同

完整cases对象仍须与a30原协议相等：N64/N256纯base、八函数/文件、100000循环、251seed oracle和原6个输出向量；六排列ABR/BRA/RAB/ARB/RBA/BAR、CPU[0,1]、jobs2，36完整编译/108native，其中36formal、72functional。每case runtime B/A≤1.10且Rust/B≥.90，完整编译wall/CPU收益另列，不能拿运行质量Pass替代编译Pass。

实际完整Verify、原模型/分区/8unit preflight、完整O2、typed预算、唯一owner/actual图/逐Use、no-cache、ordinary pinned Clang单次8对象链接、六轮同arm ELF和A/B各自materialIR一致门均继承。所有compiler/DSO/source/build/reference身份检查与原计时边界一致，ZIL/Clang23.1.4与Rustbundled23.1.1区分不变。失败先留process/raw，再验证收据/输出；缺字段、错误分支、身份变化或不足固定计数均Incomplete，不删样、不重测替换、不把阶段累计wall当关键路径可减收益。RSS仍仅wait4高水位并保留launcher/fork floor限制。

已核正式flatbinding SHA **0308c9cdba884481c08c2372e9c28b4389043db3b6aaec3bb767e71d5b5df4dd**，实际release process-attempt SHA **8b1b08310acaa9bba75a54e90e65066d48557a926b6b0e4ff8b37162a1870fb2** 和完整binding SHA **cab4b8644dc992c300f18872737663f5f8a6f4d784e32a7f6a75d3cba743c34a**；四阶段Pass，binary/source声明与freeze一致。没有重hash大image，实际prepare/screen仍须执行冻结validator的全身份核验。

实际新protocol尚待prepare后追加小文件gate。本报告不声称两个case已执行或收益已成立，不外推暖缓存、多点、并发、地址/global或生产采纳。旧a299/source/raw和NoGo结果保持完整不变。

## 实际协议追加 gate

**Pass，可执行精确协议** `out/next-optimization/full-producer-protocol-v1/protocol.json`，SHA **360635bffc5dc4230bd2c9ed2ca18e66c8edd29139e17a910afc559c03c0147f**。完整cases对象同时等于原a30和a299；640个实际源文件共729074B逐一hash匹配，9个脚本与freeze及当前字节一致。已核六排列、CPU[0,1]/jobs2、100000循环、36/108/36/72固定计数、每case1.10/.90门、unit/2+object/2 schema与身份计时合同不变。

producer继续绑定 **2458defcaf8be51c87b80c0dadf391bb585735e8 / e917926bc8976da90e58897911c72978b9a24f88**，B binary声明 **30140135c39706e8251bd5da3eb9acb4e8481a9c12acbc4aed6403f57a768097**，不被后续只加脚本的Git子提交替换。A/R/Clang工具身份声明与a299相同；A/B正式buildbinding文件SHA、source/tree/binary声明和真实release小日志SHA都一致。

实际DSO路径/resolve/SHA、LD_LIBRARY_PATH、ZIL/Clang23.1.4与Rustbundled23.1.1字段同a299。此次追加没有重读大image/DSO，没有prepare或启动编译器/LLVM/native；仍由真实runner按已冻结校验器在screen开始/结束执行全身份核验。本gate只准入该精确实验，不将未完成的质量/编译结果预判为Pass。
