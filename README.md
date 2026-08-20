# wxdb

[中文文档](README.zh-CN.md)

## Overview

`wxdb` is a standalone Windows diagnostic CLI for reading WeChat desktop
history that the operator is authorized to access. It is intentionally isolated
from SummaryAgent4GroupChat: the main project only invokes an external history
provider command and does not link, distribute, or install this project.

This project is experimental, Windows-specific, and can be used directly from
PowerShell.

## Build

```powershell
cargo build --release
```

The executable is written to `target\release\wxdb.exe`. For SummaryAgent
integration, set `[wxdb].executable` to its absolute path, or add its directory
to `PATH`.

## Quick Start

Keep the target WeChat client signed in, then inspect discovered accounts and
extract their keys:

```powershell
wxdb doctor --json
wxdb init
```

When several accounts are found, select the target account before an operation.
`WXDB_DB_DIR` is an exclusive account selector: after it is set, automatically
discovered database directories are not mixed in.

```powershell
$env:WXDB_DB_DIR = 'D:\path\to\xwechat_files\wxid_xxx\db_storage'

wxdb init
wxdb contacts --groups --limit 50
wxdb history '123456@chatroom' --limit 100 --json
wxdb export 'Group display name' --since 2026-08-01 --output .\history.json
```

`contacts` prints each group's display name and stable `@chatroom` identifier.
Use that identifier with `history` when names are duplicated or change.

## SummaryAgent4GroupChat Integration

SummaryAgent invokes `wxdb` as an external process. It does not need to copy an
executable into the application directory. Configure an absolute path so the
installed application does not depend on the interactive shell's `PATH`:

```toml
[wxdb]
executable = "D:\\codex\\wxdb\\target\\release\\wxdb.exe"
timeout_seconds = 20
history_query_timeout_seconds = 60
cache_dir = "D:\\wxdb-cache"
# When a group name changes or is ambiguous, map the wx4py display name to the
# stable identifier returned by `wxdb contacts --groups --json`.
group_name_map = { "My Group" = "123456@chatroom" }
```

SummaryAgent supplies `WXDB_CACHE_DIR` from `cache_dir` and, when configured in
the GUI, `WXDB_DB_DIR` to the child process. `WXDB_DB_DIR` selects only that
account's `db_storage` directory; it is useful when multiple WeChat accounts
exist on the machine.

The integration calls the following stable JSON interface. Compatible external
providers should accept the same flags and return an object containing a
`messages` array.

```text
wxdb history <chat> --since YYYY-MM-DD --until YYYY-MM-DD --type all --json -n <page-size>
  [--before-local-id <local-id>] [--media-decode-limit <count>]
```

`--type all` is intentional: SummaryAgent needs text, image, and voice rows so
that optional image description and voice transcription can be inserted at the
original message positions. `--before-local-id` is used for cursor pagination
when a range spans more than one page. Set `--media-decode-limit 0` to avoid
media decoding for command polling; omit it for no decoding limit, or provide a
positive bound for a summary request.

If a provider cannot complete the primary `history` call, SummaryAgent may use
the file fallback below. `export` therefore remains part of the compatibility
contract:

```text
wxdb export <chat> --since YYYY-MM-DD --until YYYY-MM-DD --format json -o <path> -n <limit>
```

## Commands

| Command | Description |
| --- | --- |
| `wxdb doctor [--json]` | Shows discovered databases, key coverage, and cache locations. |
| `wxdb init` | Scans running WeChat processes and refreshes the local key cache. |
| `wxdb contacts [--groups] [--search <text>]` | Lists contacts or groups; `--groups` limits results to groups. |
| `wxdb history <chat>` | Queries history with filtering, JSON output, cursor pagination, and a media decoding budget. |
| `wxdb export <chat> --output <path>` | Exports history as JSON. |

## Compatibility

The memory scanner attempts both modern `Weixin.exe` and legacy `WeChat.exe`.
For WeChat 4.10+ (verified with 4.1.12.55), it reads XOR-obfuscated WCDB
`Config.Cipher` objects, checking known module-relative mask locations before
falling back to the built-in mask. The older plaintext raw-key scan remains a
compatibility path. Unknown client memory layouts are reported explicitly rather
than silently producing zero keys.

## Data and Security

The keyring is intentionally stored as plaintext JSON at
`%USERPROFILE%\.wx-summary-agent\wxdb\keys.json`. Both it and the decrypted
cache contain sensitive local data; keep them on trusted disks and user
profiles only.

The first decrypted cache may approach the combined size of the selected message
shards. If space is limited, place the cache on a drive with sufficient free
space:

```powershell
$env:WXDB_CACHE_DIR = 'D:\wxdb-cache'
```
