# 177：完整泛型 State 中具体 manages 字段的整取接线

## 为什么

consumer01在泛型体Store原语处正确拒绝；转普通具体helper后的consumer02又在give ticket拒绝。只有读和整壳转交不足以组成State已有字段更新合同。该问题位于Expand符号入口，174既有Types/Analyze/独立Verify/机器字段转交已经存在。

## 改什么

仅在176的owned State callable模板内，接入对本地Package直接声明、非依赖类型的manages字段的give。先查实际Package声明、私有权限及替换后的字段类型；给实际字段建立独立已移动slot，重复整取与整壳缺字段转交拒绝，字段赋值补回才恢复。任意T、非manages字段、引用父壳和Desc局部give仍保持关闭。所有生成体仍经过普通Types、Analyze和独立Verify；Expand符号检查不能授予机器权限。

不顺带登记Store内建的泛型签名。具体state模块prepare helper返回Batch（通过collection的普通Store helper填充）{manages items:Store<Item>;ready;sum}，泛型State在成功准备后give Batch.items，从而保留替换失败时的旧State。State自身children字段取出仍直接发生在泛型函数体内。

## 怎么验

新增原生Store+Closure组合、重复give/缺字段整传/非manages/普通Desc/未调用模板负例，并由非作者核查。六文件每入口五模块消费者逐分配点失败注入；作者三片与现有泛型套件进入最终workspace回归。旧consumer01/02保持失败身份，不将改写或新接线反标到旧输入。

独审预读指出具体Batch在旧known_copy中可能经解引用伪造本地所有者；新分类按实际Package声明拒绝Copy，并递归检查tuple/array，给出未实例化模板反例。此分类不授权资源头转交，Desc local give仍关闭。
