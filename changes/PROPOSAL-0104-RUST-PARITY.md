# 104：把四目标核对落实为同语义成本证据

生产源基线固定为24ca9583d8789cfdfe3ccdbfe3eeff614bf56429；规范、公共schema和
`validation/product-gates.json`不变。此次增加普通模块、动态字节存储、拥有状态闭包
的实际ZIL/Rust消费者、分配观察和固定样本测量工具，不修改编译器。

既有Rust行为oracle的read_to_end/collect与ZIL逐字节I/O不等价于成本基线。
新基线统一C stdio、输入、输出、增长/分配失败合同，Rust使用安全Vec与普通闭包，
允许标准库ASCII优化；外部I/O的显式unsafe边界两侧均计入消费者。

测量前先提交合同与工具，完成源到O0/O2机器行为、故障分配和零遗留检查；由非作者
审查公平性后冻结所有源、输入、命令、编译器和顺序。未通过的预检原样保留。
完整进程计时包含工具启动，private edit各自使用真实持久缓存，不把ZIL进程内会话
与rustc进程外构建混比。实际结果只能证明已实现子集，不能签发完整产品Pass。

详细合同见`docs/research/104/CONTRACT.md`，机器参数见`tools/product/104/contract.json`。
