# wxdb

[English](README.md)

## 概述

`wxdb` 是一个独立的 Windows 本地诊断 CLI，用于读取操作者本人有权访问的
WeChat 桌面端聊天记录。它与 SummaryAgent4GroupChat 保持隔离：主项目只会调用
外部历史提供器命令，不会链接、分发或安装本项目。

本项目为实验性工具，仅支持 Windows，可直接在 PowerShell 使用。

## 构建

```powershell
cargo build --release
```

生成的可执行文件位于 `target\release\wxdb.exe`。若与 SummaryAgent 一起使用，
将 `[wxdb].executable` 设置为该文件的绝对路径，或将其所在目录加入 `PATH`。

## 快速开始

保持目标微信客户端已登录，然后检查发现到的账号并提取密钥：

```powershell
wxdb doctor --json
wxdb init
```

若发现多个账号，操作前务必选择目标账号。`WXDB_DB_DIR` 是排他的账号选择：
设置后不会再混入自动发现的其他数据库目录。

```powershell
$env:WXDB_DB_DIR = 'D:\path\to\xwechat_files\wxid_xxx\db_storage'

wxdb init
wxdb contacts --groups --limit 50
wxdb history '123456@chatroom' --limit 100 --json
wxdb export '群显示名' --since 2026-08-01 --output .\history.json
```

`contacts` 会输出群显示名及稳定的 `@chatroom` 标识；当群名重复或发生变化时，
请在 `history` 中使用该标识。

## SummaryAgent4GroupChat 集成

SummaryAgent 会将 `wxdb` 作为外部进程调用，无需把可执行文件复制到应用目录。
请配置绝对路径，避免已安装的应用依赖交互式 shell 的 `PATH`：

```toml
[wxdb]
executable = "D:\\codex\\wxdb\\target\\release\\wxdb.exe"
timeout_seconds = 20
history_query_timeout_seconds = 60
cache_dir = "D:\\wxdb-cache"
# 群名变化或存在歧义时，将 wx4py 中显示的群名映射到
# `wxdb contacts --groups --json` 返回的稳定标识。
group_name_map = { "我的群" = "123456@chatroom" }
```

SummaryAgent 会从 `cache_dir` 向子进程传入 `WXDB_CACHE_DIR`；在 GUI 中配置账号
目录时，还会传入 `WXDB_DB_DIR`。后者只选择该账号的 `db_storage` 目录，适合一台
电脑上存在多个微信账号的情况。

集成层会调用以下稳定的 JSON 接口。兼容的外部历史提供器应接受相同参数，并返回
包含 `messages` 数组的对象：

```text
wxdb history <chat> --since YYYY-MM-DD --until YYYY-MM-DD --type all --json -n <page-size>
  [--before-local-id <local-id>] [--media-decode-limit <count>]
```

`--type all` 是有意设计：SummaryAgent 需要文本、图片与语音记录，才能将可选的
图片描述和语音转写插回原始消息位置。`--before-local-id` 用于跨页范围查询的游标
分页。命令轮询时设 `--media-decode-limit 0` 可避免媒体解码；省略该参数表示不设
解码上限，正数则为单次总结请求设置预算。

若提供器无法完成主要的 `history` 调用，SummaryAgent 可能使用以下文件回退方式；
因此 `export` 仍属于兼容性契约：

```text
wxdb export <chat> --since YYYY-MM-DD --until YYYY-MM-DD --format json -o <path> -n <limit>
```

## 命令

| 命令 | 说明 |
| --- | --- |
| `wxdb doctor [--json]` | 显示发现到的数据库、密钥覆盖情况和缓存位置。 |
| `wxdb init` | 扫描正在运行的微信进程并刷新本地密钥缓存。 |
| `wxdb contacts [--groups] [--search <text>]` | 列出联系人或群聊；`--groups` 仅列群聊。 |
| `wxdb history <chat>` | 查询聊天记录，支持筛选、JSON 输出、游标分页与媒体解码预算。 |
| `wxdb export <chat> --output <path>` | 将聊天记录导出为 JSON。 |

## 兼容性

内存扫描器同时尝试新版 `Weixin.exe` 与旧版 `WeChat.exe`。对于 WeChat 4.1.10+
（已用 4.1.12.55 验证），它读取 WCDB 经 XOR 混淆的 `Config.Cipher` 对象：优先
检查已知模块相对 mask 位置，再回退到内置 mask。旧版未混淆 raw-key 文本扫描仍会
作为兼容路径保留。未知的客户端内存布局会清晰报为不兼容，而不会静默返回零密钥。

## 数据与安全

密钥环会按设计以明文 JSON 存储在
`%USERPROFILE%\.wx-summary-agent\wxdb\keys.json`。解密缓存与该文件都包含
敏感本地数据，请只存放在受信任的磁盘和用户配置文件中。

首次解密缓存的体积可能接近所选消息分片的总大小。空间不足时，可将缓存放到有
足够可用空间的磁盘：

```powershell
$env:WXDB_CACHE_DIR = 'D:\wxdb-cache'
```
