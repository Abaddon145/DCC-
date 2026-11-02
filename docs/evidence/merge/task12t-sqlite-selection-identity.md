# SQLite 选择必须匹配登记的来源

2026-10-30。合成来源选择会话除校验素材库、来源 profile 和 epoch 外，还对所选文件句柄核对持久登记的文件身份；读取前后再次核对。复用 `SqliteSelection` 的同一校验函数。没有绑定、选到另一个文件或文件变成硬链接时拒绝读取。身份检查不赋予访问授权。

以两个临时 SQLite/WAL 数据库复现修复前的错误来源接受问题；修复后拒绝错配、未登记文件及选择后新增硬链接。学习迁入专项 154 项通过，日志 `release-artifacts/yingliu-merge/sqlite-selection-identity-green.log`。完整后端回归 741 通过、2 忽略，日志 `release-artifacts/yingliu-merge/sqlite-selection-identity-full.log`；随后追加的读取期间新增硬链接断言单独通过，日志 `release-artifacts/yingliu-merge/sqlite-selection-identity-final.log`。本轮未修改前端，未重新运行前端测试或构建。

此处选择会话和快照仍仅供测试，没有接入真实源文件对话框或生产读取。检查结果不能证实文件内容未被原位替换，也不能解决快照期间所有并发变化。测试用只读检查回调不得执行暂存或提交：回调后的失效检查无法撤销已经写入的结果。未来接入写操作时必须在事务边界内实现授权复核与互斥，而不是依赖事后检查。
