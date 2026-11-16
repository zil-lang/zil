# 稳定等价类与缓存编码整链

本批从df06acabf续作；最后正式测量源码仍4a23edc7，不能给本批新代码分配旧性能数字。目标按绝对等待处理保留缓存重启、5.1k连续私改及check大图；完整链修改后集中验收，不逐改跑性能矩阵。

后端以完整normalized Function Eq形成纯DAG类，用完整结构指纹选择稳定实现符号；遇任何Function/Global符号碰撞整次回退，不以hash替代Eq。原定义、地址用途、保护角色与ABI保留；新增实际定义作为私有class_symbols随同TransformProduct进入原四槽完成链，未建额外持久授权或无界历史。helper数量、实际成本及模型项数有界，不增加原source-copy预算。

实际类实现进入当前完整CGU导入闭包与原复制上限；新域失败就丢弃新类，从当前原始机器输出重新运行旧变换/原planner，尝试耗时留在请求内。原call-free/collected快速路径保留。内化以后原逐unit可达裁剪仍在；无引用的本次类prototype另做完整原模型检查后清理，避免仅声明名变化污染无关对象。普通声明、全O2、机器/API/LLVM检查及当前Verify不减。

短函数使用既有FunctionSession完整上下文/真实CFG/原六费用重放协议，移除>=4statement的调度收益门；foreign/life域仍走原完整Builder，scalar/签名/限额条件保留。目的是修复大批短体在重启时完全无缓存资格及失去canonical编码owner，而不是放宽语言预算。

semantic publication在完整当前raw值与候选/limits匹配后复用已编码分段，磁盘输入仍是非授权候选；具体恢复、取消、未知wire版本及限额边界由同批独审与原始反例检查。没有把缓存hash或字节当作Verify能力。

Astra按当前源码复核完整imports、碰撞、call-free、模型扩张和取消边界，并对照固定Rust58cf的稳定CGU和work-product实现。必要回归包括代表退出但实际调用不变的16unit全值相等、真实callee变化仅改变依赖单位、新域复制超限完整回退、global碰撞无修改、恶意无用prototype不被裁剪掩盖、短体磁盘CFG实际命中与私改失效。

当前4a23的check5000wide仍是热编辑领先、冷编译落后：八次私改1.353/Rust2.792s，冷编译1.622/.807s，重启1.385/.027s。不能把这些与原生5.1k的相反排序混成整体反转。

旧结果记忆核对：bcf的5.1k重启后首改1.223/Rust1.285s领先，但重启+首改3.948/1.304s仍落后；4a23同格1.245/.995s来自不同窗口，Rust对照也变化，不据此直接声称算法反转。进程内无修改、连续私改、进程重启、重启首改与完整流程分别比较。check5000wide冷编译当前仍落后Rust，不能用原生5.1k冷编译领先替代。

资源准备只对固定df06完整远端归档、已fresh恢复且本次重新逐文件hash相等的旧blobs证据副本退役；原rows/response/summary/用户cache保留，SDK和源码不动。这不是产品盘量节省；具体ledger随本批证据保全。旧全量恢复仍在df06的8包。

状态：源码链施工完成待冻结；尚无本批新正确性、release或比较性能结论。后续只对冻结源码统一运行受影响门、原生运行质量和actual-main/candidate/Rust同O2/CGU16/jobs1窗口；保留全部负结果，不合main，研究开关不默认启用。


后续同批增加三条实际优化：checked-add按完整当前Function/声明/trap/triple复用已完成规划输出，仍做当前完整原模型preflight；call-class按当前完整header域、完整函数值、实际callee类名依赖复用地址/规范化/hash工作，selector仅定位、完整Eq定类，随原四槽有界保存；wire单次完整解码同时给出真正消费的函数/plan字节边界、共享编码和真实node费用，恢复不再逐函数重解码/整体probe，也不把解码留给首改。

原62d第一次统一收据机械检查与backend305通过，Types108过/3失败已保留。三失败来自短体缓存覆盖变化后的旧调度计数，以及life域主动不入缓存后旧fixture强制unwrap；所有fresh结果、原始六费合计、预算错误、成功epoch保留断言不减。Life反例现在额外断言无CFG候选可编码，并继续精确高/低工作边界。尚未启动任何新性能矩阵。完整新增逻辑收齐后再统一验收，而非逐改测试。

类域也显式绑定到PartitionInput；每unit裁剪只绑定该unit实际类声明位置，而非用全局类集变化否决所有单位，当前真实声明角色变化不能借旧prune产物跨域复用。

404a已补齐真实class layout的成功发布及全值helper/纯DAG/ABI/imports/DCE/物理符号核算，backend314、publication1、strict/fmt/release和22tool tests通过。retain03在404a完成72请求/24功能格/10运行质量门，check02完成102请求；native03在1k第五私改发现两份历史layout物理不可区分，仍拒绝，不当全面成功。首改实际15objects hit/1miss，但5.1k真实重启+首改5.023/Rust1.385s仍落后，数值只属于404a该窗口。

现一次贯通真正planner receipt key→PartitionProduct/四槽历史→CodegenSummary→native inventory/retained inventory/ELF/汇总；不再从缓存目录猜layout。实际class域缺key/缺真实producer保守拒绝，原完整layout/closure/ABI/O2/运行门保持。真实Source回归含多次私改、分区及最终文件命中、删除产物后的恢复；本新整链尚待统一验收及新性能窗口。

初始8窗与所有首败、旧编译器和已过门的完整raw已fresh验证85,083files、4,411,100,174raw bytes、1,204,793,250archive bytes，位于evidence/stable-class-initial-progress-20261116；这不是新key源码的性能验收。SDK18/native-link沿原恢复入口，不重建LLVM。
