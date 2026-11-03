# 328：结构化线程的真实 pthread 后端

实现325已冻结的 `WithScopedThread { worker, argument, body }`：只在
`x86_64-unknown-linux-gnu` 使用实际 pthread_create/pthread_join，结果顺序为
`(created, worker_result, body_result)`。沿现有机器模型同时交给LLVM API及文本
参考消费者，不以测试C代码承担生产线程行为。

每个调用位置生成私有helper及pthread start入口。helper栈上固定context保存
已初始化i32参数和独立i32 worker结果；pthread_t按本目标的64位unsigned long
单独保存。child只读参数、调用已核静态worker并写自己的结果。父线程独立消费
body并清剩余环境，成功join之后才读child结果；create/join提供真实POSIX同步。
create失败只清理已准备body环境，返回false/0/0，不调用body或同步fallback。
join失败调用abort并unreachable，不能返回释放仍可能被访问的context。

body沿既有inline环境、按指针/按值调用ABI和closure drop helper；不复制到child。
线程helper引用和静态worker边纳入CGU及layout缓存输入；pthread链接仅新增能力
出现时启用。公共schema/Wire、资格证明及CT关闭由root/325负责，本单不制造权限。

限定验证复用backend lib测试模块：机器IR结构及LLVM核验、真实OS线程执行、
成功/创建失败/加入失败和拥有环境清理、API/reference O0/O2及分区依赖。
真实源消费者待325组合后执行；手造已核前IR模型只按模型证据报告。

本单不自审后端，不宣称Suspend/SABI、Mutex、取消、一般Transfer/Share或产品
四目标完成。原生Windows、Rust运行/内存/维护/编译成本比较未测。
