use anyhow::Result;
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{
    VirtualQueryEx, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOCACHE, PAGE_READONLY, PAGE_READWRITE,
    PAGE_WRITECOMBINE, PAGE_WRITECOPY,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

use super::{collect_db_salts, KeyEntry};

const HEX_PATTERN_LEN: usize = 96;
const CHUNK_SIZE: usize = 2 * 1024 * 1024;
const CONFIG_CIPHER_NAME: &[u8] = b"com.Tencent.WCDB.Config.Cipher";
const CONFIG_XOR_MASK: [u8; 32] = [
    0xd2, 0xc7, 0x44, 0x24, 0x58, 0x02, 0x00, 0x00, 0x00, 0x48, 0x89, 0x44, 0x24, 0x50, 0x48, 0x8b,
    0x45, 0x00, 0x48, 0x84, 0x4c, 0x24, 0x48, 0x48, 0x89, 0x44, 0x25, 0x40, 0x48, 0x58, 0x4c, 0x24,
];

pub fn scan_keys(db_dir: &Path) -> Result<Vec<KeyEntry>> {
    let db_salts = collect_db_salts(db_dir);
    let pids = find_wechat_pids();
    if pids.is_empty() {
        anyhow::bail!("找不到 Weixin.exe 进程，请确认微信正在运行");
    }
    let mut raw_keys = Vec::new();
    let mut open_errors = Vec::new();
    for pid in pids {
        let process = match open_readable_process(pid) {
            Ok(process) => process,
            Err(error) => {
                open_errors.push(format!("pid {pid}: {error}"));
                continue;
            }
        };
        for pair in scan_memory(process)
            .into_iter()
            .chain(scan_wcdb_config_keys(process))
        {
            if !raw_keys.contains(&pair) {
                raw_keys.push(pair);
            }
        }
        unsafe {
            let _ = CloseHandle(process);
        }
    }

    if raw_keys.is_empty() && !open_errors.is_empty() {
        anyhow::bail!(
            "OpenProcess 全部失败；如微信权限较高，请用相同或管理员权限运行: {}",
            open_errors.join("; ")
        );
    }
    if raw_keys.is_empty() {
        anyhow::bail!(
            "未在微信进程内存中找到数据库密钥模式；当前客户端版本可能尚不兼容，请保持微信登录后重试"
        );
    }

    let mut entries = Vec::new();
    for (key_hex, salt_hex) in &raw_keys {
        for (db_salt, db_name) in &db_salts {
            if salt_hex == db_salt {
                entries.push(KeyEntry {
                    db_name: db_name.clone(),
                    enc_key: key_hex.clone(),
                    salt: salt_hex.clone(),
                });
                break;
            }
        }
    }
    if entries.is_empty() {
        anyhow::bail!(
            "在微信进程内存中找到了 {} 个候选密钥模式，但没有与目标数据库盐匹配的密钥；请确认该账号对应的微信客户端仍在运行",
            raw_keys.len()
        );
    }
    Ok(entries)
}

fn open_readable_process(pid: u32) -> windows::core::Result<HANDLE> {
    let access_modes = [PROCESS_VM_READ | PROCESS_QUERY_INFORMATION, PROCESS_VM_READ];
    let mut last_error = None;
    for access in access_modes {
        match unsafe { OpenProcess(access, false, pid) } {
            Ok(handle) => return Ok(handle),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(windows::core::Error::from_win32))
}

fn find_wechat_pids() -> Vec<u32> {
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Vec::new();
    };
    let mut entry = PROCESSENTRY32 {
        dwSize: std::mem::size_of::<PROCESSENTRY32>() as u32,
        ..Default::default()
    };
    let mut pids = Vec::new();

    unsafe {
        if Process32First(snap, &mut entry).is_err() {
            let _ = CloseHandle(snap);
            return pids;
        }
        loop {
            let name = std::ffi::CStr::from_ptr(entry.szExeFile.as_ptr()).to_string_lossy();
            if is_wechat_executable(&name) {
                pids.push(entry.th32ProcessID);
            }
            if Process32Next(snap, &mut entry).is_err() {
                break;
            }
        }
        let _ = CloseHandle(snap);
    }
    pids
}

fn is_wechat_executable(name: &str) -> bool {
    name.eq_ignore_ascii_case("Weixin.exe") || name.eq_ignore_ascii_case("WeChat.exe")
}

/// Extracts WeChat 4.10+ keys from WCDB's obfuscated Config.Cipher values.
///
/// The configuration map stores a `[pointer, length]` string key inside its
/// node, with the cipher value reachable from the node's config pointer. The
/// value is XOR-obfuscated with a fixed mask and decodes to SQLCipher's raw
/// `x'<key><salt>'` syntax.
fn scan_wcdb_config_keys(process: HANDLE) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let names = find_bytes(process, CONFIG_CIPHER_NAME);
    if names.is_empty() {
        return results;
    }

    for name_addr in &names {
        let mut pair = [0u8; 16];
        pair[..8].copy_from_slice(&(*name_addr as u64).to_le_bytes());
        pair[8..].copy_from_slice(&(CONFIG_CIPHER_NAME.len() as u64).to_le_bytes());
        for string_ref_addr in find_bytes(process, &pair) {
            let Some(node) = read_memory(process, string_ref_addr.saturating_sub(0x10), 0x50)
            else {
                continue;
            };
            let Some(node_name_ptr) = read_u64(&node, 0x10) else {
                continue;
            };
            if !names.contains(&(node_name_ptr as usize))
                || read_u64(&node, 0x18) != Some(CONFIG_CIPHER_NAME.len() as u64)
            {
                continue;
            }
            let Some(config_ptr) = read_u64(&node, 0x28).filter(|ptr| is_plausible_ptr(*ptr))
            else {
                continue;
            };
            let Some(value) = read_memory(process, config_ptr as usize + 0x88, 0x28) else {
                continue;
            };
            let (Some(data_ptr), Some(data_len)) = (read_u64(&value, 0x8), read_u64(&value, 0x10))
            else {
                continue;
            };
            if !is_plausible_ptr(data_ptr) || !(99..=1024).contains(&data_len) {
                continue;
            }
            let Some(blob) = read_memory(process, data_ptr as usize, data_len as usize) else {
                continue;
            };
            decode_wcdb_config_blob(&blob, &mut results);
        }
    }
    results
}

fn decode_wcdb_config_blob(blob: &[u8], results: &mut Vec<(String, String)>) {
    let decoded: Vec<u8> = blob
        .iter()
        .enumerate()
        .map(|(index, value)| value ^ CONFIG_XOR_MASK[index % CONFIG_XOR_MASK.len()])
        .collect();
    search_pattern(&decoded, results);
}

fn read_u64(buf: &[u8], offset: usize) -> Option<u64> {
    buf.get(offset..offset + 8)?
        .try_into()
        .ok()
        .map(u64::from_le_bytes)
}

fn is_plausible_ptr(ptr: u64) -> bool {
    (0x1_0000..0x8000_0000_0000).contains(&ptr)
}

fn scan_memory(process: HANDLE) -> Vec<(String, String)> {
    let mut results = Vec::new();
    let mut addr: usize = 0;

    loop {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let ret = unsafe {
            VirtualQueryEx(
                process,
                Some(addr as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if ret == 0 {
            break;
        }

        let region_size = mbi.RegionSize;
        let base = mbi.BaseAddress as usize;
        if mbi.State == MEM_COMMIT && is_writable_readable_page(mbi.Protect.0) {
            scan_region(process, base, region_size, &mut results);
        }

        addr = base.saturating_add(region_size);
        if addr == 0 {
            break;
        }
    }

    results
}

fn find_bytes(process: HANDLE, needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > CHUNK_SIZE {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut addr = 0usize;
    loop {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        let ret = unsafe {
            VirtualQueryEx(
                process,
                Some(addr as *const _),
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if ret == 0 {
            break;
        }
        let base = mbi.BaseAddress as usize;
        if mbi.State == MEM_COMMIT && is_readable_page(mbi.Protect.0) {
            find_in_region(process, base, mbi.RegionSize, needle, &mut hits);
        }
        addr = base.saturating_add(mbi.RegionSize);
        if addr == 0 {
            break;
        }
    }
    hits
}

fn find_in_region(process: HANDLE, base: usize, size: usize, needle: &[u8], hits: &mut Vec<usize>) {
    let overlap = needle.len().saturating_sub(1);
    let mut offset = 0usize;
    while offset < size {
        let chunk_size = CHUNK_SIZE.min(size - offset);
        let addr = base + offset;
        if let Some(buf) = read_memory(process, addr, chunk_size) {
            let mut position = 0usize;
            while let Some(relative) = buf[position..]
                .windows(needle.len())
                .position(|candidate| candidate == needle)
            {
                let index = position + relative;
                hits.push(addr + index);
                position = index + 1;
            }
        }
        offset += if chunk_size > overlap {
            chunk_size - overlap
        } else {
            chunk_size
        };
    }
}

fn is_readable_page(protect: u32) -> bool {
    let base = protect & !(PAGE_GUARD.0 | PAGE_NOCACHE.0 | PAGE_WRITECOMBINE.0);
    matches!(
        base,
        value
            if value == PAGE_READONLY.0
                || value == PAGE_READWRITE.0
                || value == PAGE_WRITECOPY.0
                || value == PAGE_EXECUTE_READWRITE.0
                || value == PAGE_EXECUTE_WRITECOPY.0
    )
}

fn is_writable_readable_page(protect: u32) -> bool {
    let base = protect & !(PAGE_GUARD.0 | PAGE_NOCACHE.0 | PAGE_WRITECOMBINE.0);
    matches!(
        base,
        x if x == PAGE_READWRITE.0
            || x == PAGE_WRITECOPY.0
            || x == PAGE_EXECUTE_READWRITE.0
            || x == PAGE_EXECUTE_WRITECOPY.0
    )
}

fn scan_region(process: HANDLE, base: usize, size: usize, results: &mut Vec<(String, String)>) {
    let overlap = HEX_PATTERN_LEN + 3;
    let mut offset = 0usize;
    while offset < size {
        let chunk_size = std::cmp::min(CHUNK_SIZE, size - offset);
        let addr = base + offset;
        if let Some(buf) = read_memory(process, addr, chunk_size) {
            search_pattern(&buf, results);
        }
        offset += if chunk_size > overlap {
            chunk_size - overlap
        } else {
            chunk_size
        };
    }
}

fn read_memory(process: HANDLE, addr: usize, len: usize) -> Option<Vec<u8>> {
    if addr == 0 || len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    let mut bytes_read = 0usize;
    let ok = unsafe {
        ReadProcessMemory(
            process,
            addr as *const _,
            buf.as_mut_ptr() as *mut _,
            len,
            Some(&mut bytes_read),
        )
        .is_ok()
    };
    if !ok || bytes_read != len {
        return None;
    }
    Some(buf)
}

fn search_pattern(buf: &[u8], results: &mut Vec<(String, String)>) {
    let total = HEX_PATTERN_LEN + 3;
    if buf.len() < total {
        return;
    }
    let mut idx = 0;
    while idx + total <= buf.len() {
        if buf[idx] != b'x' || buf[idx + 1] != b'\'' {
            idx += 1;
            continue;
        }
        let hex_start = idx + 2;
        if !buf[hex_start..hex_start + HEX_PATTERN_LEN]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
            || buf[hex_start + HEX_PATTERN_LEN] != b'\''
        {
            idx += 1;
            continue;
        }
        let key_hex = String::from_utf8_lossy(&buf[hex_start..hex_start + 64]).to_lowercase();
        let salt_hex = String::from_utf8_lossy(&buf[hex_start + 64..hex_start + 96]).to_lowercase();
        if !results
            .iter()
            .any(|(key, salt)| key == &key_hex && salt == &salt_hex)
        {
            results.push((key_hex, salt_hex));
        }
        idx += total;
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_wcdb_config_blob, is_wechat_executable, CONFIG_XOR_MASK};

    #[test]
    fn accepts_new_and_legacy_wechat_process_names() {
        assert!(is_wechat_executable("Weixin.exe"));
        assert!(is_wechat_executable("wechat.EXE"));
        assert!(!is_wechat_executable("WeChatApp.exe"));
    }

    #[test]
    fn decodes_obfuscated_wcdb_raw_key_literal() {
        let plain = format!("x'{}{}'", "a".repeat(64), "b".repeat(32));
        let blob: Vec<u8> = plain
            .bytes()
            .enumerate()
            .map(|(index, value)| value ^ CONFIG_XOR_MASK[index % CONFIG_XOR_MASK.len()])
            .collect();
        let mut pairs = Vec::new();

        decode_wcdb_config_blob(&blob, &mut pairs);

        assert_eq!(pairs, vec![("a".repeat(64), "b".repeat(32))]);
    }
}
