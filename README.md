# wxdb

Standalone local diagnostic CLI for reading a user's own WeChat desktop history.
It is intentionally separate from SummaryAgent4GroupChat: the main project only
invokes an external history-provider command and neither links nor distributes
this code.

This project is experimental, Windows-specific, and intended only for data the
operator is authorized to access. It is not published as a crate or bundled into
the SummaryAgent installer.

## Build

```powershell
cargo build --release
```

Set SummaryAgent's `[wxdb].executable` to the resulting absolute path, or add
the binary directory to `PATH`.
