# 217：同步框架语言起步消费者

这是五模块普通 ZIL 消费者，核对现有语言能否共同承载节点数据、两个不同的
拥有回调、事件循环、ID 关系、中间删除、父关系修改、回调替换与关闭。它不实现
ZUI 框架，也不把同步起步范围等同用户全部 ZUI 需求。

- `factory.zil`：F 为拥有完整 Product 的 FnMut，每次将 child 取到局部、修改并
  在成功或错误返回前完整回填；G 为拥有另一 ByteBuffer 的 FnOnce。
- `state.zil`：完整 Node<F,G>，保存真实两个环境及节点/父 ID。
- `collection.zil`：普通 T:StoreElement 的 seed/push/pop/release/erase。
  中间删除先准备临时后缀 Store，移动后缀、关闭目标、再移回；没有新语言权限。
- `main.zil`：运行时输入选择 0/1/2/3 轮、删除/重挂接、同类型新环境替换、
  未调用退出与容量满 push 的完整错误值清理。
- `portable.zil`：显式同步 C 字符 I/O。

字节输入 `0`–`4` 选择固定场景，非法字节和 EOF 返回 9。资源标记区分节点壳
`A/B/C/D`、Product 叶 `a/b/c/d/i` 和另一环境 `1/2/3/4`。`S . D R E > Z F`
是构造、轮次、删除、替换、提前退出、FnOnce、完成和失败标记。

固定 oracle 在 `tools/language/217/ORACLE.json`，由独立的源操作模型生成，
在五模块源首次机器执行之前提交。它核每个真实申请失败点、下一个非失败控制、
实际分配/释放、峰值存活分配个数和最终 live0；请求 payload 峰值字节只记录，
不冒称 RSS 或 Rust 对比。

`indexed_control/main.zil` 保存审计先验证的独立索引表示控制。其先前真实结果
是 O0/O2 的 `B!AC`，与新五模块预声明模型分开记账。

执行参数与精确成果见 `docs/ZUI-LANGUAGE-READINESS-217.md` 以及
`evidence/impl/217/`。必须使用干净提交、冻结 CLI 构建收据和一个全新的输出目录。
运行器不会构建 Rust，也不重跑正式整仓。
