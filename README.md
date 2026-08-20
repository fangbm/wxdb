# wxdb

Standalone local diagnostic CLI for reading a user's own WeChat desktop history.
It is intentionally separate from SummaryAgent4GroupChat: the main project only
invokes an external history-provider command and neither links nor distributes
this code. It can also be used directly from PowerShell.

This project is experimental, Windows-specific, and intended only for data the
operator is authorized to access. It is not published as a crate or bundled into
the SummaryAgent installer.

## Build

```powershell
cargo build --release
```

Set SummaryAgent's `[wxdb].executable` to the resulting absolute path, or add
the binary directory to `PATH`.

## Usage

Keep the target WeChat client logged in, then diagnose the discovered accounts:

```powershell
wxdb doctor --json
wxdb init
```

When multiple accounts are present, pin the intended account before every
operation. `WXDB_DB_DIR` is an exclusive account selection, so automatic
discovery will not mix in other accounts.

```powershell
$env:WXDB_DB_DIR = 'D:\path\to\xwechat_files\wxid_xxx\db_storage'
wxdb init
wxdb contacts --groups --limit 50
wxdb history '123456@chatroom' --limit 100 --json
wxdb export '群显示名' --since 2026-08-01 --output .\history.json
```

`contacts` lists a group's stable `@chatroom` identifier as well as its display
name. Use the identifier with `history` when names are duplicated or change.

The keyring is stored as plaintext JSON at
`%USERPROFILE%\.wx-summary-agent\wxdb\keys.json` by design. Treat that file
and the decrypted cache as sensitive local data. The memory scanner attempts
both current `Weixin.exe` and legacy `WeChat.exe`. For WeChat 4.10+ it also
reads WCDB's XOR-obfuscated `Config.Cipher` objects (verified with 4.1.12.55);
the older plaintext raw-key scan remains as a fallback. A client whose in-memory
key layout is unknown is reported as unsupported instead of silently succeeding.

The initial decryption cache can be close to the combined size of the selected
message shards. Put it on a drive with sufficient free space when necessary:

```powershell
$env:WXDB_CACHE_DIR = 'D:\wxdb-cache'
```
