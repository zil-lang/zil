# 183 独立审核

裁决 Pass，绑定作者源码 `19f58c16ed800b0cee5a8afcc11908925920cd63`。审核者 `/root/restore177_inventory` 非作者；独立树只增加测试 `f35a8be1d092ade47a5b9a20e3af273a457b682d`，生产差异为空。

独立 target 实际运行 95 组、0 失败、0 忽略，包含新增 3 组边界。新增正例在 O0/O2 共 12 次实际执行，覆盖从两个输入载体选择一个成功返回、另一载体按序清理，以及先消费输入成功值再返回错误的零成功责任。混入新建分支不能把另一输入分支伪造成 create；成功/失败工厂后的 raw/Wire 重复 DropErrorUnion 均拒绝。原159探针字节保留，并随作者原测试重新执行。

新构建 CLI 从四个实际模块编译并执行 Mark、ByteBuffer、Store<Item>。Linux 62 次与 Wine 62 次全部匹配预先给定的输出、退出及分配 oracle；各含36次分配/增长失败。成功和失败、直接丢弃及调用方错误都无剩余分配，free 与分配数相等。Wine 为兼容层，未执行原生 Windows。普通程序保持编译器原输出；计数探针对真实 Linux realloc/free 或 Win64 Heap 外部调用做明确测试插桩。

源码检查确认显式拥有模式只作用于成功载荷；失败分支没有该资源的 fresh 标记，输入成功值仍保留 incoming 身份。独立 Verify 校验精确 tag、一次 Take、缺失与重复清理；后端只读实际活动 payload，先清 live 标记再调用 teardown。公开 schema22及外层普通Value移动规则保持。

冷启动 handoff 原始失败保留，涉及历史规范登记及其它正在施工任务的快照；不把它写成 Pass。此裁决仅接受183精确候选，不替代 root 对 schema23/182 合流的整仓回归修复，也不声明完整语言、产品或性能终态。
