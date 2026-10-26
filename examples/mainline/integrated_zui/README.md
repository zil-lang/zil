# 组件底座组合消费者

五模块普通ZIL程序，组合固定独立ByteBuffer字段借用（120）、跨模块名义泛型callable类型（121）、纯CT与runtime Drop共存（122）、signed shift（124）、工厂正文helper与拥有handler（125）、四次FnMut预热（126）。两个运行时组件状态独立；main同时持有p.b借片并替换p.c。

`zil build main.zil --entry value --opt O0 -o program --json`；O2同理。预期输出 `RRT46` 后接换行：两个Model和外层Token分别只Drop一次，然后输出结果46。同字段反例把 `let old = give p.c;` 换为 `let old = give p.b;`，必须在Verify以E-BORROW-CONFLICT拒绝。实际集成收据绑定受测提交，见 `evidence/integration-20261026-draft24/summary.json`。

这是ZUI所需语言能力的组合例，不是完整ZUI框架、文本排版或GPU实现；没有运行/内存/Rust性能结论。
