# 完整O2 canonical producer / actual Required分支：独立草案

独立新ROOT，测量器代码已冻结，未prepare、未生成数据、未编译/执行。manual-prelink a299冻结源码及913raw保持原样：该版两格运行Pass、完整编译NoGo。本草案只为下一单改动对照，不跨session偷算收益或覆盖旧结果。新candidate source `2458defcaf8be51c87b80c0dadf391bb585735e8` / tree `e917926bc8976da90e58897911c72978b9a24f88`，binary `targets/manual-full-2458/release/zil` SHA `30140135c39706e8251bd5da3eb9acb4e8481a9c12acbc4aed6403f57a768097`；flatbinding为 `out/next-optimization/binding-manual-full-2458.json`。Root已完成API64/proof6/no-native/release，原ce90681f的2321二进制保留。没有执行本轮screen，也不复用旧2321二进制身份。

## 固定范围

完全复用原a30 cases对象、源字节、251seed数学oracle、100000循环、00/ff功能与41正式输入，N64/N256纯base。六排列ABR/BRA/RAB/ARB/RBA/BAR、36完整编译、108native=72功能+36formal；B/A≤1.10、Rust/B≥.90和全部长尾保持。A main990、R Rust1.99.1；新B source/binary另外pin。

四工具与实际LLVM DSO/LD_LIBRARY_PATH身份合同原样继承：A/B/R所选compiler每request前后在计时外对称核SHA，screen前后完整核所有工具、DSO、源码、bindings与reference。ZIL/Clang/nativeLLVM23.1.4，Rust1.99.1 bundledLLVM23.1.1明确分开；不引入旧wrapper首1GB哈希。完整actual compiler进程计时内保留实际语言/机器/API验证。

## 唯一新管线及准确字段

producer `zil-research-manual-prelink-unit/2` 必须是完整`default<O2>`，正optimize_ns、当前structured preflight、实际before/after verify、ordinary-no-summary及全actualcanonical/definitions/declarations/direct_calls/intrinsics。原typedComplete owner/Allowed/每函数与总费用界、当前Verify/原模型/分区/8unit预检、sealedbitcode与实际逐Use检查不变，summary/index/cache仍关闭。

harness独立从八unit实际图重算transitive Required，不能用Allowed是否为空选分支。post `zil-research-manual-prelink-object/2` 必须actual_required_path_verified=true，精确required/allowed/imports/donor数。对象pipeline必须是：

`canonical-full<O2>|budgeted-direct-imports=private|empty-required=emit|nonempty-required=thinlto<O2>|merge-functions=after|aliases=false`

- actual Required为空：postlink_optimization_ran=false，object.optimize_ns=research.postlink_optimize_ns=research.trim_link_ns=0，donor_modules=0/imports=[]。仍实际parse/check_sealed/完整图/owner/budget与LLVM验证，直接emit当前完整O2canonical module。没有把空路径称为trim/link/private都执行。
- actual Required非空：postlink_optimization_ran=true，object.optimize_ns和research.postlink_optimize_ns相等且>0，trim_link_ns>0；保留原完整donor parse/trim/link/private与thinlto<O2>，Required必须在原Allowed内。

新API的`actual_required_path_verified`替换旧/1的`verify_parse_trim_link_private_postlink`，旧manual/1验证器不改。输出八个object不等于八次第二O2；harness单列actual_postlink_optimization_units。完整producer O2不能降成薄prelink或仅靠bool声称已跑，实际pipeline/正计时/验证字段都强核。

## 退出与未执行

同原driver，raw metrics先写后parse，fatal/timeout/缺字段/错误branch/身份变化保存Incomplete且不运行native，不删失败重测；每arm六轮ELF/materialIR须各自一致，跨arm允许不同但真实运行门全过才谈质量。最终普通pinclang-O2链接，禁止旧ThinLTO/link plugin参数。只是针对当前两个纯base的研究harness，不扩大到地址/global、多点/并发、原八case、总体权重或产品采用。

本次仅做AST静态读取及已存在buildbinding/日志SHA核对，测量器源码freeze供Root随后prepare。新API源码和成功buildbinding已固定，非作者测量器审查及实际protocol身份核验后进入真实执行；当前没有质量或性能结果。
