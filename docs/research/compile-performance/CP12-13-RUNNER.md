# CP12 / CP13 的显式配对配置

`rust_bench.py` 的新计划 schema 为 `zil-rust-compile-performance/2`。旧 `/1` 计划、旧 runner 提交和历史成绩保持原样；新 runner 不接受旧计划重算。新增两个默认 false 的 prepare 开关，均保存到 plan 和 summary，原始编译 argv 与完整报告仍保留。

| prepare 选项 | 完整构建的实际行为 | 必须核对的报告 |
| --- | --- | --- |
| `--query-cache` | 只允许 `--cache-mode incremental`；在每个独立 sequence 的 work/queries 下启用 CP12，四个请求共享该目录 | cold/private/public 必须 `miss` 且成功 stored；unchanged 必须 `hit` 且未重写；key 为完整 hex，CT 三项 usage 必须存在；bypass、corrupt、写入放弃、缺失报告均不能评分 |
| `--closed-executable` | 向 ZIL build 显式传无值同名开关；允许 CP13 在 Linux 完整可执行构建内部化符合条件的 ZIL 生成符号 | `native_codegen.closed_executable=true` 且 `internalized_functions` 为明确的非负整数；缺字段、静默未应用或与计划不同均不能评分 |

未请求前端缓存时，报告必须 disabled，或来自尚无 CP12 字段的旧精确二进制。未请求 closed-executable 时，新报告必须 false 且内部化计数为 0；旧二进制可缺该新增字段。无论配置如何，每个 ZIL 完整请求仍必须 Success、`verification_runs=1`、timings 关闭、LLVM API 后端，以及原对象缓存命中边界。Rust 命令行、默认 CGU、目标、checked arithmetic、std/linker 身份和独立 oracle 不变。

## 新计划示例

在干净的 runner checkout、完整工具链环境与四个 CPU 可用时：

```sh
python3 tools/research/compile-performance/rust_bench.py prepare \
  --out /tmp/new-cp12-o0-incremental-plan \
  --zil /absolute/frozen/zil --zil-source EXACT_COMMIT \
  --rustc /absolute/rustc --clang /absolute/clang \
  --tool /absolute/libLLVM.so \
  --profile O0 --cache-mode incremental --query-cache \
  --units 8 --jobs 4 --cpus 0,1,2,3 --blocks 3
```

完整五语料保留默认 case 选择；单语料 smoke 才加 `--case` 并保留 selected-original-weight 的部分覆盖标识。CP13 新配置显式加 `--closed-executable`；这不等于已证明 O2 运行质量，仍需 `measure-runtime` 与原始 oracle 的独立核对。CP12 不允许用 cold 标签请求查询缓存；冷构建完全不请求 source/object/query/Rust incremental 缓存。

后续 preflight、measure、measure-runtime、summarize 命令和原工作流一致。编译使用 plan.cpus，原 runtime 工具将程序固定到 plan.cpu（第一个 CPU）；不把整个运行器进程先限制成一个 CPU。ABBA/BAAB、三个或五个区块、四请求和五语料权重、warmup 标记、完整 build / runtime 边界都未改变。

## 验证范围

21 项运行器控制通过，包括原 17 项身份冻结、失败/缺行禁止评分和编译/runtime 分离控制，以及新增的显式模式、前端缓存状态/用量/写入失败、closed-executable 实际报告检查。新增控制是合成记录的测量工具测试，不是编译器性能证据。CP12 精确 release 最大语料的真实检查点和运行 oracle 在 `CP12-QUERY-CACHE.md` 另有功能 smoke；正式采样与计划由协调者重新冻结。
