# 完整 Product 的共享读取

`main.zil` 把实际内联 `Root` 借给 `value(&Root)`，递归读取子包的 Copy 字段和用户资源头。最后一次读取后取走 `root.child`，实际输出 `XAB`，退出码为零。拥有资源仍各自清理一次，借入没有复制、搬走或装箱整个 Product。

`allocation.zil` 把真实 ByteBuffer 放入 Product，再共享读取其他字段。正常输出 `XA`；第一次分配失败输出 `F`、退出 10；第二次分配失败输出 `GA`、退出 11。测试在实际 LLVM API 对象的链接中使用既有 allocator probe，检查各条路径的真实分配、释放与 `live=0`。

`active_move_reject.zil` 在共享借入仍有后续读取时移动 root；应在 Verify 以 `E-BORROW-CONFLICT` 拒绝，不生成可执行文件。

配置本机 Rust/LLVM 工具链后，在仓库根运行：

```sh
python3 tools/language/224/validate.py --out /tmp/zil-224-fresh-run
```

输出目录必须是新目录。限定测试显式选择 LLVM API，包含 O0/O2、源码正反例、原始候选与 Wire、各阶段预算临界值。它不替代独立审核或整仓验收。
