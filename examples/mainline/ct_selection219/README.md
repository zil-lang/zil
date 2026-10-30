# 源级配置选择消费者

`main.zil`、`config.zil`、`io.zil` 组成三个普通模块。`config_wide.zil` 是另一份冻结配置输入，runner 将它原字节复制为另一个测试项目的 `config.zil`。

`config.compact()` 实际调用纯标量 `area` helper。条件选择普通声明和语句列表；初始化的局部 ByteBuffer 在分支后继续使用，Marker 与 buffer 的清理归接收块。运行输入 0–3 控制提前返回、增长、第二缓冲区和普通非零结果；分配失败经真实 enum 错误联合传播。

标准 runner：

```sh
python3 tools/language/219/consumer.py --repo /path/to/zil --cli /path/to/zil-cli --compiler-build-receipt /path/to/compiler-build.json --out /path/to/new-run
```

runner 要求 clean 提交和精确 CLI 构建收据，并在任何运行前写入独立 oracle。它构建两种配置的 O0/O2 普通程序，再用已有 allocator observer 注入每个实际分配点及 last+1；不会从观测输出反推预期。

精确受测身份、成功和历史失败见 `docs/LANGUAGE-219.md` 与 `evidence/impl/219/final/RESULT.json`。这是语言前置消费者，不是 ZUI 框架实现。
