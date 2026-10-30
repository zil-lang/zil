# 阶段原生产物独立归档

91609072原始修补及本地回执不改写。后续独立复核发现：默认阶段归档失败时，共用native-evidence根会让继续运行的参考阶段有机会覆盖旧文件；ZIP归档亦需额外保存文件权限，并禁止大小写折叠重名造成漏件。因此在启动最终完整CI前进一步收紧归档边界。前一位reviewer已承认初审漏检跨阶段写入，不将旧“无阻塞”意见冒作本版审查。

workspace与reference各自写入独立子目录，分别capture、upload、release。释放只限当前phase，重新检查完整相对路径、SHA-256、字节数和mode；新增、缺失、内容或权限改变均阻断。capture拒绝symlink及casefold冲突；上传包括隐藏文件。所有测试、features、限制、no-fail-fast和45分钟时限保持。

本地控制已证明隐藏文件和755权限进入清单、mode/content变化阻止释放、两阶段互不删除、空阶段合法、Case/case冲突与symlink拒绝。由于下载的ZIP可能以普通文件权限展开，重放原生程序前应按对应小receipts中的phase-native-manifest.json逐项校验相对路径/字节/SHA-256，再恢复清单的mode；清单同时保存原runner root用于身份追溯，不要求在原绝对目录重放。

91609072触发的临时run仅作被后续提交替代的尝试记录，不计为最终完整验证。最终Pass仍须来自后续完整GitHub运行。

固定依赖源码依据：

- https://github.com/actions/upload-artifact/blob/cf430e030ddbb5b0abf93d22962f4752f3646cd9/README.md#permission-loss
- https://github.com/actions/upload-artifact/blob/cf430e030ddbb5b0abf93d22962f4752f3646cd9/src/shared/search.ts
- https://github.com/actions/upload-artifact/blob/cf430e030ddbb5b0abf93d22962f4752f3646cd9/action.yml
- https://github.com/actions/upload-artifact/blob/cf430e030ddbb5b0abf93d22962f4752f3646cd9/src/shared/upload-artifact.ts
