# wxdb

## 概述 / Overview

`wxdb` 是一个独立的 Windows 本地诊断 CLI，用于读取操作者本人有权访问的
WeChat 桌面端聊天记录。它与 SummaryAgent4GroupChat 保持隔离：主项目只会调用
外部历史提供器命令，不会链接、分发或安装本项目。

`wxdb` is a standalone Windows diagnostic CLI for reading WeChat desktop
history that the operator is authorized to access. It is intentionally isolated
from SummaryAgent4GroupChat: the main project only invokes an external history
provider command and does not link, distribute, or install this project.

本项目为实验性工具，仅支持 Windows，可直接在 PowerShell 使用。

This project is experimental, Windows-specific, and can be used directly from
PowerShell.

## 构建 / Build

```powershell
cargo build --release
```

生成的可执行文件位于 `target\release\wxdb.exe`。若与 SummaryAgent 一起使用，
将 `[wxdb].executable` 设置为该文件的绝对路径，或将其所在目录加入 `PATH`。

The executable is written to `target\release\wxdb.exe`. For SummaryAgent
integration, set `[wxdb].executable` to its absolute path, or add its directory
to `PATH`.

## 快速开始 / Quick Start

保持目标微信客户端已登录，然后检查发现到的账号并提取密钥：

Keep the target WeChat client signed in, then inspect discovered accounts and
extract their keys:

```powershell
wxdb doctor --json
wxdb init
```

若发现多个账号，操作前务必选择目标账号。`WXDB_DB_DIR` 是排他的账号选择：
设置后不会再混入自动发现的其他数据库目录。

When several accounts are found, select the target account before an operation.
`WXDB_DB_DIR` is an exclusive account selector: after it is set, automatically
discovered database directories are not mixed in.

```powershell
$env:WXDB_DB_DIR = 'D:\path\to\xwechat_files\wxid_xxx\db_storage'

wxdb init
wxdb contacts --groups --limit 50
wxdb history '123456@chatroom' --limit 100 --json
wxdb export '群显示名' --since 2026-08-01 --output .\history.json
```

`contacts` 会输出群显示名及稳定的 `@chatroom` 标识；当群名重复或发生变化时，
请在 `history` 中使用该标识。

`contacts` prints each group's display name and stable `@chatroom` identifier.
Use that identifier with `history` when names are duplicated or change.

## 命令 / Commands

| 命令 / Command | 说明 / Description |
| --- | --- |
| `wxdb doctor [--json]` | 显示发现到的数据库、密钥覆盖情况和缓存位置。 Shows discovered databases, key coverage, and cache locations. |
| `wxdb init` | 扫描正在运行的微信进程并刷新本地密钥缓存。 Scans running WeChat processes and refreshes the local key cache. |
| `wxdb contacts [--groups] [--search <text>]` | 列出联系人或群聊；`--groups` 仅列群聊。 Lists contacts or groups; `--groups` limits results to groups. |
| `wxdb history <chat>` | 查询聊天记录，支持 `--since`、`--until`、`--type`、`--limit` 和 `--json`。 Queries history; supports `--since`, `--until`, `--type`, `--limit`, and `--json`. |
| `wxdb export <chat> --output <path>` | 将聊天记录导出为 JSON。 Exports history as JSON. |

## 兼容性 / Compatibility

内存扫描器同时尝试新版 `Weixin.exe` 与旧版 `WeChat.exe`。对于 WeChat 4.10+
（已用 4.1.12.55 验证），它读取 WCDB 经 XOR 混淆的 `Config.Cipher` 对象：优先
检查已知模块相对 mask 位置，再回退到内置 mask。旧版未混淆 raw-key 文本扫描仍会
作为兼容路径保留。未知的客户端内存布局会清晰报为不兼容，而不会静默返回零密钥。

The memory scanner attempts both modern `Weixin.exe` and legacy `WeChat.exe`.
For WeChat 4.10+ (verified with 4.1.12.55), it reads XOR-obfuscated WCDB
`Config.Cipher` objects, checking known module-relative mask locations before
falling back to the built-in mask. The older plaintext raw-key scan remains a
compatibility path. Unknown client memory layouts are reported explicitly rather
than silently producing zero keys.

## 数据与安全 / Data and Security

密钥环会按设计以明文 JSON 存储在
`%USERPROFILE%\.wx-summary-agent\wxdb\keys.json`。解密缓存与该文件都包含
敏感本地数据，请只存放在受信任的磁盘和用户配置文件中。

The keyring is intentionally stored as plaintext JSON at
`%USERPROFILE%\.wx-summary-agent\wxdb\keys.json`. Both it and the decrypted
cache contain sensitive local data; keep them on trusted disks and user
profiles only.

首次解密缓存的体积可能接近所选消息分片的总大小。空间不足时，可将缓存放到有
足够可用空间的磁盘：

The first decrypted cache may approach the combined size of the selected message
shards. If space is limited, place the cache on a drive with sufficient free
space:

```powershell
$env:WXDB_CACHE_DIR = 'D:\wxdb-cache'
```
