# 181 独立审核

结论：公共 IR、目标约束与 Windows Store 平台片在精确作者提交 `8493aca5ce86a77cd5e9ea8ab3d3629bafafa2a6` 上 Pass。审核测试提交为 `7d9a288483384797dab358de1d8c27cb9042cb26`，两者差异仅为新增独立测试。生产源码没有由审核者修改。

## 实际执行

最终限定测试为 37 / 0 / 0：31 个公共 IR、3 个作者目标测试、3 个独立目标及 Heap ABI 测试。另以独立 target 构建 CLI，Linux 与独立 Wine prefix 各执行 8 次，分别包含 2 次普通程序、6 次探针程序及其中 4 次真实分配失败注入。每个平台均覆盖 O0/O2；Wine 执行的文件经 PE 签名、AMD64 machine 和 PE32+ 头检查。没有把作者的更大矩阵累计为独立执行次数。

独立攻击覆盖 4 个伪造目标的 raw/Wire 两路、23 个旧/未知 Wire 版本、核验后的 LIR 目标篡改，以及 3 个 Heap 外部符号签名冲突。Store-only 源不含 ByteBuffer，仍核对真实 Heap ABI 声明和调用，不能靠 ByteBuffer 恰好触发运行时输出来掩盖遗漏。

## 保留的失败

`frozen01` 的公共 IR 两个失败确为作者旧断言迁移遗漏；作者 8493aca 修复后重跑为 31 / 0 / 0。`final02` 的独立 LIR 测试失败来自审核者写错期望诊断集合，实际 `E-CODEGEN-UNBOUND-LIR` 已拒绝伪造；只修审核测试断言后 `attacks03` 与 `final04` 均通过。所有首次日志保留，不改写为通过。

一次可选 `llvm-readobj` 文件头检查因私有工具链未安装该命令而 exit127；没有据此声称工具输出。实际 PE 检查改用 Python 只读真实二进制头，详见 `pe-inspection.json`，执行产物不收录。

## 代码与归因

平台生产改动仅把两个 Store 目标准入点改为精确的受支持 x64 LLVM profile。HeapAlloc/HeapReAlloc/HeapFree 的调用与失败行为来自既有 checked runtime；本片没有重写 allocator。依据 Microsoft 原始文档核对 HANDLE/指针64位、DWORD/BOOL32位、SIZE_T64位，分配 flags 为0；增长失败保留旧存储及其元素。当前支持的有限布局不超过8字节对齐，满足 Windows Heap16字节对齐，不能把该结论推广至任意 over-aligned 类型。

该审核不覆盖 root 另行合流的 178–181 组合执行；也不把 Wine 当作原生 Windows 认证。完整语言 NotSatisfied、产品 NotRun、B0 Doing；Rust 四目标与净突破未由这些有限测试完成。

只收录源夹具、命令、诊断、观测、哈希和 raw/Wire 候选输入；不收录 PE/ELF/object/LLVM、完整阶段 dump 或缓存。`manifest.json` 校验本目录其他文件。
