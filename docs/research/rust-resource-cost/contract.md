# 104：同语义Rust成本合同

本合同在任何性能采样前登记。阈值只读`validation/product-gates.json`；没有新增
较宽门槛。原始生产编译器固定24ca源，ZIL编译器本身以release构建。现有比较
不声称涵盖Rust/ZIL全部语言，且完整产品验收仍须满足四目标合取与至少一项净突破。

## 同一工作与合理Rust

| 消费者 | 必须完成的工作 | 存储/清理 |
|---|---|---|
| stream | 跨模块逐字节ASCII大写，原样保留其余字节 | 无应用动态存储 |
| buffer | 收完全部输入后原地ASCII大写，再逐字节输出 | 动态头从0开始，8后倍增；结束释放 |
| retained | 保存FnMut，非零字节追加历史，以sum、len和实际历史字节共同产生输出；零字节重置并输出0 | State拥有唯一buffer；重置释放旧存储，闭包退出释放末批 |

保留闭包必须读取历史，不能仅维护可替代整个容器的sum/len。双方同用libc
getchar/putchar；输入的任何负返回视为结束，忽略putchar返回值（这个有限I/O
合同不声称覆盖读写失败）。分配失败退出2，已写前缀相同，拥有者仍须清理。
固定输入下sum、len以及sum+len+history均不溢出i64，容量翻倍前使用相同上界。Rust用Vec、try_reserve_exact和
普通拥有闭包，允许to_ascii_uppercase/make_ascii_uppercase的库优化；不为制造
优势改用裸指针容器。除了与ZIL显式unsafe FFI对应的I/O，没有应用unsafe存储操作。

Rust设置edition2024、opt-level=0/2、overflow-checks=yes、panic=abort、debuginfo=0、
codegen-units=1，默认目标CPU、无LTO；ZIL用同机native目标与O0/O2。固定Rust1.99.1
内部LLVM23.1.1和Clang23.1.5的版本差异公开记录。两侧均允许现有编译器优化，
不以“相同LLVM版本”替代实际测量。ZIL调试构建尚无调试信息，此处debug build明确
表示无优化、无调试符号的开发构建；不能外推为带完整调试信息的产品比较。

## 行为、资源、输入

机器合同列出固定10个长度、O0/O2和全部失败位置。Python独立参考函数不调用任一
被测程序；两个语言侧程序与模型的stdout和退出状态逐字节匹配。输入由固定整数公式
生成，保留实际字节和SHA，包含0、ASCII、非ASCII和容量边界。

独立C观察器仅用于额外的非计时构建，链接wrap malloc/calloc/realloc/free；在
run入口启用、run返回（包括局部/闭包已销毁）后结束。记录实际分配请求、规范化
逻辑allocation ID、容量、成功/失败、释放和最终live/peak字节。成功realloc可
改变物理地址，地址不跨进程比较；原allocation ID继续代表同一个逻辑buffer。
忽略没有分配的空容器free(null)，不把它算有效释放。故障只发生在预登记第N个请求，
拒绝申请必须保留旧拥有者直到清理。若N超过实际请求数，明确记failure_triggered=false，
只作为无故障行为检查，不能算故障路径覆盖。观察器不得进入计时binary；它的结论也不替代
独立Verify或Roam的通用故障发现能力。RSS包含完整进程，分配观察只覆盖显式run。

## 编译与运行测量

每单元2对预热、30对保留，15对ZIL先、15对Rust先，以种子104冻结顺序。运行主要
单元是三个O2消费者×64KiB/4MiB。无探针binary的启动、程序执行及退出都计入，
输出到普通文件，逐次核对SHA。p95采用nearest rank，另报中位、全部原始值及RSS。

编译比较三消费者×三种流程：

- cold_check：全新ZIL进程check至独立Verify，对rustc --emit=metadata；输出目录空。
- cold_debug_build：全新进程、空项目缓存，实际O0源到本机可执行文件，包括后端/链接。
- private_edit：双方先在相同路径完成O0构建；在已有且会执行的私有函数内做等价体修改，
  保持接口不变，再以新进程完成O0构建。Rust启用自己的-C incremental缓存；ZIL使用
  --cache-dir的未核源码候选。首次预构建另保留原始成本，但不计入第二次编辑成本。

Rust用直接rustc入口，避免把Cargo协调启动成本强加给对照。ZIL的磁盘候选缓存
不等于模块级增量编译；若其整项目重建仍然更慢，将直接报告。源码复制、体修改和
输出核对在计时之外，两侧均如此。原程序/修改程序及缓存输入身份保留；private edit
不追加无用函数。所有build产物另执行小输入并核对输出，避免计时接受无效产物。

计时用perf_counter_ns覆盖Popen到wait4，文件预先打开；限时用进程级alarm，
超时终止本次所属进程组并保留失败。记录user/sys、exit、wall、ru_maxrss和完整命令。
ru_maxrss是内核报告的最大单进程峰值，不是编译器/链接器并发内存之和；本轮不会
把它写成全流程总内存。运行单进程RSS与应用实际请求的峰值/归零观察分别报告。

root协调独占CPU2窗口，无其他施工构建/压力任务。系统其他负载和OS页缓存不受
本项目完全控制；cold表示空会话/项目缓存，标准库/toolchain文件可在页缓存中。
完整固定样本只运行一次；失败或噪声不足保留结论，不删慢样本、不追加直到通过。
报告每单元两个中位之比及配对比值中位、精确二项中位区间（描述性逐单元95%，
没有家族同时覆盖保证），所有点估计都保留。突破只称已实现三消费者的候选证据。

执行器固定实际rustc、clang和C链接驱动的绝对入口，记录其文件SHA与工具环境，
定时前核对全部输入/二进制/顺序。总结必须逐条匹配完整注册矩阵与顺序，缺整个
单元、重复pair/language、非零退出或错合同均拒绝。已存在的输出目录只能拒绝，
不能在异常处理时覆盖旧结果；派生摘要与原始进程原件分开。

## 表达与终态

本片不以行数或符号少证明维护更简单。六个演进族全部进入缺口表；有真实消费者的
layout_change/static helper只能算部分能力，不能冒充helper_generic_dyn全族。
尚未实现的图删除、并发、挂起取消等写明责任/依赖，不从产品义务中移除。
