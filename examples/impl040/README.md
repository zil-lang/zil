# 无捕获闭包与函数指针

`main.zil` 在 `fn` 上下文中推导闭包参数和结果，组合函数返回、函数实参、记录、数组、tuple、条件选择与局部重绑。每个转换后的闭包生成真正的无环境函数入口，没有堆盒、trampoline 或隐式环境。

```sh
cargo run --offline --locked -p zil-cli -- run examples/impl040/main.zil --entry result
cargo test --offline --locked -p zil-driver --test impl040
rustc --edition=2024 examples/impl040/reference.rs -o out/impl040-reference
./out/impl040-reference
```

ZIL 的 `result` 与配对 Rust 程序都打印 `5000000042`；普通 ZIL `main` 退出码为 `0`。Rust 程序仅作同任务行为对照，没有调用它检查 ZIL，也未用此样例宣称产品性能优势。

9 项专项测试覆盖既有 CL13/CL14、Module 工厂、Unit 与零长数组 ABI、记录更新、投影赋值、混合函数编号、独立 verify 伪造环境拒绝和预算。实际 ZIL 程序均由源码经 TypedCFG 与独立 verify 生成 LLVM，在 O0/O2 执行。当前转换以明确 `fn` 上下文中的闭包字面量为范围；先绑定为具体 closure 类型后再转换、一般闭包逃逸、间接调用的地址来源合同仍不在本片。
