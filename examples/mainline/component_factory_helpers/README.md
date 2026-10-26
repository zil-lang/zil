# 用普通 helper 准备并返回组件状态

这个四模块消费者让组件工厂内部的两种普通高阶调用真正组合：先通过共享初始化 helper 得到初始状态，再通过可变 helper 预热已经创建的 handler，最后把拥有 `Model` 的 handler 返回调用者。两个实例在多次交错事件后分别保留自己的状态。

在仓库根目录构建与执行：

```sh
cargo run --locked -p zil-cli -- build examples/mainline/component_factory_helpers/main.zil --entry value --opt O0 -o /tmp/component-factory-helpers
/tmp/component-factory-helpers
```

成功输出是 `IIRR63` 加换行，退出码 0。`I` 表示两次工厂返回前各自清理初始化闭包持有的临时资源；`R` 表示调用者退出时两个保留的组件状态各清理一次。数值 63 来自两个种子先加 2、再分别预热加 3，随后交错执行 `+2`、`+3`、`+1`。测试直接读取这四个源文件，在 O0 和 O2 执行并核对实际公开 callable 合同。

这里解除的是普通语言组合限制：使用者可以把初始化和预热算法写成 helper，不必搬到工厂外。返回环境仍必须自包含；工厂递归、工厂转发、借入捕获逃逸、helper 循环及超过现有有限调用次数不在这个切片中。这个消费者不声明 OS 回调生命周期、完整 ZUI 框架或相对 Rust 的运行与编译性能已验收。
