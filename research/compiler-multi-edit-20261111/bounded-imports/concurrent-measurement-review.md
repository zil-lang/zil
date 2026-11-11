# 1679并发screen：独立冻结审核

结论：**最终并发脚本静态审核Pass；只有精确运行质量门已实际完成且全部通过，才允许prepare或run。** 未prepare、import/执行脚本、构建或采样；当前尚未声称运行质量门已经通过，也未声称并发screen已经执行。

精确文件concurrent_screen.py SHA256：`de17216895cbd48e29adb463a90b3c94fe6a8a68bf1b36768c488289283759ea`，已实算一致。共同依赖screen.py为`8cc25d67e4e575f4fb4b0a2f0035c82e1904d96c4309f52d608b3b6bb77612c3`，fixture.py为`b4d588232df9b0d41d8877adcf99c712a90cb8f931adcf991dce85510ca7f314`；prepare将其和并发脚本本身的精确哈希写入plan，run重算。

## 原阻断已经由实际逻辑关闭

初稿只有execution_prerequisite文字，没有run-time gate核验，不能执行。最终版加入必需--runtime-gate，并在创建fixture目录之前调用validate_runtime_receipt；run在创建输出、编译或原生调用之前再核同一固定receipt。不是靠root人工记得先验收或相信一个Pass字符串。

validator读取真正完成的summary及旁边protocol.json，固定summary路径/哈希、协议哈希且要求summary.protocol_sha256一致；协议引用的原corpus也重新核哈希。要求format/status Pass、gate_pass、所有native有效、24clean/216native/144formal、8个精确且唯一的N64/256×base/k1/k16/k64 case、failed_cases为空、未删除长尾。完整A/B/R二进制path+SHA256、source claims、clang及reference身份都与当前plan一致，二进制实际字节还重哈希。

每case每arm的wall/CPU/RSS必须各有六个有限正数，均值由六个原值重新算并核对，不过滤缺值；重新计算B/A<=1.10与Rust/B>=.90，同时要求summary内ratio与重算值一致。原运行协议format/policy、cases、source/binaries/clang也检查。失败、未完成、错case、换工具或被改写的receipt会在准备/执行前拒绝，不能用编译收益覆盖运行质量失败。

## 并发与own-clean边界

固定首轮N256、blocks1、include-rust、k16/64、c1/2/4与ARBBRA。总CPU0..3随c均分为不重叠slice，ZIL jobs=4/c、8CGU；Rust继承相同slice并正常incremental。每arm-position、每candidate的source/output/object/semantic或Rust incremental缓存独立；salt1..4生成不同最终字节，独立数值oracle与实际改动文件差集检查保持。

每candidate clean现在使用**同arm编译器**，不是B去匹配A代码。最终每arm的ELF/materialIR强制等于自己的clean oracle；跨arm差异只记录分类，没有不正确地要求相等。每份ZIL clean/seed/final仍执行共享validate_zil的完整O2、当前Verify1、实际N、当前unit preflight与LLVM核验门，原生输出按独立oracle检查；runtime前置才是真正的运行性能门，三输入功能输出不能冒充它。

首轮共有36最终批次、84最终child；每child还有一次clean和一次seed，共252编译请求，三输入功能核验完整执行时为756个。不同c有不同工作量，不把c4批次wall当作与c1相同工作量的单请求延迟比。

## 失败与计时

batch仍先启动全部child再wait4回收；没有poll/communicate提前消费usage。raw stdout/stderr及batch-attempt finally保存保持，后者记录已回收、未启动、unfinished、cleanup rows/CPU/errors；clean/seed/final raw-metrics先于validate。外层finally即使验证或同码比较失败，也会写complete=false与failure并保留未筛选观测，不把未完成格计通过。

已用AST只读比较旧审通过的79f9并发脚本：sha/save/helpers/vectors/candidate/command/batch/validate核心函数完全相同；新增核心逻辑是严格runtime receipt gate、同arm clean与跨arm身份分类。未用该比较替代对新增validator的逐项实审。

CPU和RSS口径保持：CPU是wait4各child与OS记账后代之和，RSS为最大单进程值而非并发总峰；wall含launch skew。并发screen不生成原请求频率score、不宣称运行Rust90或父门完成。

最终结论：上述hash可在真实runtime Pass之后继续prepare/run；如runtime任一格失败则本screen保持未执行，不能通过修改摘要或放宽门强行进入。
