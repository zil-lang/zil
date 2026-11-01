# 保持入口块身份的 JumpThreading 插件对照

真实 ca79 scalar8192/default-after 的一次 API pass timing 与自身进程用户态采样仍显示 JumpThreading 1.6067 CPU 秒；549样本中152同时含 JumpThreading→MergeBasicBlockIntoOnlyPred→DTU::recalculate→CalculateFromScratch，JT共157。前一43037样本也有相同路径。因此本候选针对已实证的入口块替换导致完整支配树重算，不泛归因于共享trap，也不关闭JumpThreading。

研究仅在独立 opt 进程加载 PassPlugin，不修改生产 API、默认策略、LLVM动态库或扩大生产 C++/unsafe 边界，不全量重建LLVM。固定官方LLVM23.1.4 commit `7a941dfb2877a0b8cf30323fefa85ad019e0b0f5` 的JumpThreading.cpp/.h保留许可证与原文SHA。复制类和私有类型命名空间，隔离cl::opt/debug/statistic名称，注册原版clone与entry-preserving两个新pass。原版臂保留上游算法，先对内建pass检验结果一致，不能把重编/host与API差异算作修复收益。

官方opt静态包含LLVM，而本机libLLVM.so由同一官方PIC archives重链接。插件引用opt导出的C++符号，不链接第二套libLLVM，防止双runtime/重复option registry。开发头必须来自同一23.1.4官方SDK（含准确generated config/intrinsic头），不使用当前仓库的23.1.5开发包。仅构建有限插件，至多2jobs；ABI、assertions、RTTI、标准库ABI、host/compiler/headers/.so身份进入收据。

修正严格限于原本可合并且唯一前驱就是函数entry的情况，使用现有MergeBlockIntoPredecessor维持原entry对象。双侧LVI先失效；维护LoopHeaders集合；utility更严格的资格拒绝时回退原路径。新方向删除的是正在处理的BB，因此不能只换一行：修正版按每轮函数顺序建立WeakTrackingVH工作列表，RAUW将当前handle转到存活entry，立即继续原while处理；删除会清空handle，避免Lazy DTU flush后悬空。新建块在Changed后的下一轮处理。原版clone保留原循环。若只等待下一轮重访entry，可能将DT重算的二次成本转移到全函数轮询，因此必须测试大链的工作次数和真实时间。

两臂使用同一精确目标、完整展开O2管线，仅替换全部JumpThreading节点。实际输入从经当前Verify/机器预检的真实CGU单元捕获，不能用public program.ll替代多CGU输入。opt输出再用相同的禁止重复LLVM IR优化的Clang/codegen路径发射、链接；两臂缓存/失败发布边界相同，完整pipeline/plugin/tool/input identities进入研究key。该层级对照不冒充生产LLVM API已经实现。

先验证no-op插件ABI、原版clone与内建pass一致、LLVM IR/支配树/保留分析验证，再覆盖可折叠/不可折叠链、循环/PHI/地址取用、unreachable、guard/assume与non-returning行为、资源/闭包/FFI。真实源码编译与opaque运行质量/尺寸必须对照，错误输入仍当前拒绝，不替换旧产物。功能窗口用于正确性与归因，正式收益需要另行排他窗口。所有失败、超时、NotRun及工具障碍均保留，不只报告改善样本。
