# 132：拥有UTF8文本的事务替换（候选，未独立审核）

## API与机制

`edit.replace(source, begin, end, insertion, byte_limit) -> Edit`：两个输入为共享字节切片；位置为Unicode标量序号，区间半开。必须完整验证旧文本和插入文本，再验证位置和长度上限，最后分配新ByteBuffer，按前缀—插入—后缀准备。没有就地改旧头/旧载体。调用者读公开Copy状态后，用`edit.take(result)`消费结果包取得唯一拥有缓冲区，成功时显式替换自己的字段；失败结果为空，旧文本保持。

`result(give)`是把输入Edit中的已有责任移交出去，不是制造新责任；`take`不得错误声明`result(create)`。manages字段仍私有，不为库便利放宽封装或借用规则。

```zil
let result = edit.replace(bytes_view(state.data), begin, end,
                          bytes_view(insertion), byte_limit)
let code = result.code
let prepared = edit.take(result)
if code == 0 { state.data = give prepared }
```

0成功；1旧文本无效；2插入无效；3标量区间无效；4大小上限/长度溢出；5分配失败；6原语合同异常。无效输入的error给出首个不合法序列起始字节。错误优先级按完整旧文本、完整插入、位置、大小、分配固定。

## 验证与范围

真实`.zil`经归档生产编译器ff0ddb7e、schema17、独立Verify生成LLVM，Clang17构建O0/O2；另外两种O0/O2链接测试专用realloc/free计数与失败注入shim。Python严格UTF8解码/字符串切片是不同实现的行为参考。测试包含NUL、组合字符、emoji/ZWJ、标量边界、代理码点、过长编码、截断和随机字节。C仅为故障注入/生命周期观察，不实现文本库。

旧视图在替换以后再用的最近反例，由真实Verify以E-BORROW-CONFLICT拒绝。输入分配失败、插入分配失败和结果分配失败均留有实际机器退出、分配/释放计数；成功、拒绝和失败路径检查live=0。测试输入协议以16位字节长度和序号承载有界样本，库本身使用i64。

实现为普通ZIL库，不新增编译器原语/语法/隐式RC或全局表。不等于完整标准库String：没有封闭不可伪造UTF8类型、字素簇/规范化、rope、原地删除/容量复用、普通泛型Vec或异步并发。复制为O(n+m)，峰值包含旧输入与新缓冲，不能宣称Rust级运行性能突破。

本次未重建最新draft24编译器。归档编译器源身份早于124–127，虽功能子集相容，但不能把这里的成绩升级为最新workspace、正式门或独立审核通过。失败试验保留；待另一执行环境在锁定Rust工具链下重建与复核后再合入。

## 重放

```sh
python3 tools/research/132/native_test.py --driver /path/to/123-BASE-plain \
  --clang /path/to/clang --out /new/empty/132-output
```

`--driver`是123保存的编译会话实验可执行文件，不是任意同名zil CLI；其SHA与源身份见receipt。独立恢复过程可从`evidence/impl/123/raw-confirmation.tar.gz`提取`forward002/bin/BASE-plain`并核对SHA；不从不可信下载站取得二进制。最新编译器环境应直接用同一源样本与自己的命令重测，不能仅改receipt中的身份。
