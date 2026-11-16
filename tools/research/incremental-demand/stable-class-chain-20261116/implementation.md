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
