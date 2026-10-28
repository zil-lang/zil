# UTF8 标量区间编辑

`utf8.zil`验证整个输入并定位标量边界；`edit.zil`含复制版`replace`和原地版`replace_in_place`；`main.zil`为测试用二进制帧stdin消费者，不是交互编辑器。

新原地API、错误顺序、别名/分配/能力边界与复跑命令见`../../../docs/IMPL139-OWNED-TEXT.md`。

`replace_in_place`的目标为借入ByteBuffer描述，调用不取走责任。共享插入切片必须与目标分离。错误保持输入不变；缩短/等长不分配。使用Unicode标量索引，不做grapheme或归一化。
