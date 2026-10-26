# 多模块整数小程序

程序计算 `gcd(252, 105)`、小于 1000 的素数个数与素数总和，并检查空边界。
三个源码文件分别负责入口、Euclid 算法和素数计算。`math.gcd` 使用普通文件入口，
`math.primes` 使用显式目录入口；目录不会自动导入其它文件。

```sh
cargo run --offline --locked -p zil-cli -- build examples/numberlab/main.zil -o out/numberlab
./out/numberlab
cargo run --offline --locked -p zil-cli -- run examples/numberlab/main.zil --entry result
```

独立程序的退出码应为 **0**，任何自检不符返回 1。最后一条命令通过已有的
显式 entry 观测入口打印完整整数 **76316**：21 + 168 + 76127。
所有算法和控制流由三个 ZIL 源文件编译成原生代码；打印返回值的是驱动生成的
观测入口，这不代表 ZIL 的字符串或标准 I/O 已实现。

本例覆盖点路径、文件/目录模块、公开与私有函数、直接调用、局部变量、
半开整数范围 `for value in 2..limit`、`while`、分支、短路条件、整数除余和检查算术。
`for` 的上下界在进入循环时依次求值一次，迭代变量只读，空或逆向范围不会执行循环体。
它不使用尚未完成的资源、聚合或闭包，
也不证明这些语言能力已经通过验收。参数选在 i32 范围内；任意更大输入仍受
检查算术规则约束。GCD 示例的约定输入为非负整数。
