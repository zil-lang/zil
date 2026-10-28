# 147：有限本地名义 trait

为什么：Zig/ZUI报告指出一般组合能力缺口，不能持续以专门的字节API替代。此切片支持本地具体普通结构体声明所实现的操作合同，不引入运行时。

是什么：`trait Measure { fn width(self:&Self)->i64; }` 加 `impl Measure for Label { fn width()->i64=>.columns*8 }`。可以继承只读/可变/按值receiver、定义返回Self的静态工厂；完整检查方法集、模式和签名。相同trait可以由多个具体类型实现。

怎么做：在Expand先检查有限声明，再复用现有名义方法降为普通函数；Types、Analyze、独立Verify、Lower和LLVM维持原边界。不生成vtable，不把用户trait当Copy/Share/Transfer许可。

不支持：泛型trait/泛型约束、默认方法、关联类型、supertrait、dyn、跨模块trait解析、Managed/Package目标和冲突方法限定。现有闭包推导的实例点调用还有缺口，示例使用同语义已支持的 `Type.method(value,...)`；不把这一绕行称已解决实例方法推导。

消费者 `examples/mainline/static_traits/main.zil` 组合只读测量、可变定位和ByteBuffer，输出512个矩形、16384字节，检查末矩形(960,744,24,16)。这是无窗口布局验证，不是完整渲染器。

旧receiver_regression中“所有trait均Unsupported”是有意扩展的接受性边界；改动前提见PROPOSAL-0147。安全、词法错误和预算边界的旧预期没有放宽。
