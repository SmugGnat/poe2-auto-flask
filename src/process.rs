use std::ffi::c_void;
use std::io;
use std::mem::{size_of, zeroed};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW,
    MODULEENTRY32W, PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
};

const TARGET_PROCESS_NAMES: &[&str] = &["PathOfExileSteam.exe", "PathOfExile.exe"];
const SCAN_CHUNK_SIZE: usize = 1024 * 1024;
const STILL_ACTIVE_EXIT_CODE: u32 = 259;

#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub base_address: usize,
    pub size: u32,
}

#[derive(Debug, Clone)]
pub struct PeSection {
    pub name: String,
    pub address: usize,
    pub size: usize,
}

pub struct Process {
    pid: u32,
    handle: OwnedHandle,
    main_module: ModuleInfo,
}

impl Process {
    pub fn attach_to_poe2() -> io::Result<Self> {
        let pid = find_process_id(TARGET_PROCESS_NAMES)?;
        let handle = open_read_only(pid)?;
        let main_module = find_main_module(pid, TARGET_PROCESS_NAMES)?;

        Ok(Self {
            pid,
            handle,
            main_module,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn main_module(&self) -> &ModuleInfo {
        &self.main_module
    }

    pub fn is_alive(&self) -> io::Result<bool> {
        let mut exit_code = 0u32;
        let ok = unsafe { GetExitCodeProcess(self.handle.raw(), &mut exit_code) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(exit_code == STILL_ACTIVE_EXIT_CODE)
    }

    pub fn validate_pe_headers(&self) -> io::Result<()> {
        let base = self.main_module.base_address;
        let pe_offset = self.pe_header_offset()?;

        let signature = self.read_exact::<4>(base + pe_offset)?;
        if signature != *b"PE\0\0" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "PE signature is invalid",
            ));
        }

        Ok(())
    }

    pub fn find_pe_section(&self, target_name: &str) -> io::Result<PeSection> {
        let base = self.main_module.base_address;
        let pe_offset = self.pe_header_offset()?;
        let file_header = self.read_exact::<20>(base + pe_offset + 4)?;

        let section_count = u16::from_le_bytes([file_header[2], file_header[3]]) as usize;
        let optional_header_size = u16::from_le_bytes([file_header[16], file_header[17]]) as usize;

        if section_count == 0 || section_count > 96 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("implausible PE section count: {section_count}"),
            ));
        }

        let section_table = base + pe_offset + 4 + 20 + optional_header_size;
        for index in 0..section_count {
            let header = self.read_exact::<40>(section_table + index * 40)?;
            let name_end = header[..8].iter().position(|&byte| byte == 0).unwrap_or(8);
            let name = String::from_utf8_lossy(&header[..name_end]).to_string();

            if name != target_name {
                continue;
            }

            let virtual_size = u32::from_le_bytes(header[8..12].try_into().expect("fixed slice"));
            let virtual_address =
                u32::from_le_bytes(header[12..16].try_into().expect("fixed slice"));
            let raw_size = u32::from_le_bytes(header[16..20].try_into().expect("fixed slice"));
            let size = virtual_size.max(raw_size) as usize;

            if size == 0
                || virtual_address as usize >= self.main_module.size as usize
                || virtual_address as usize + size > self.main_module.size as usize
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("PE section {name} has an invalid virtual range"),
                ));
            }

            return Ok(PeSection {
                name,
                address: base + virtual_address as usize,
                size,
            });
        }

        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("PE section {target_name:?} was not found"),
        ))
    }

    pub fn scan_pattern(
        &self,
        start: usize,
        size: usize,
        pattern: &[Option<u8>],
    ) -> io::Result<Vec<usize>> {
        if pattern.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "pattern must not be empty",
            ));
        }
        if size < pattern.len() {
            return Ok(Vec::new());
        }

        let overlap_len = pattern.len().saturating_sub(1);
        let mut overlap = Vec::<u8>::new();
        let mut offset = 0usize;
        let mut matches = Vec::new();

        while offset < size {
            let read_len = SCAN_CHUNK_SIZE.min(size - offset);
            let chunk = self.read_bytes(start + offset, read_len)?;

            let mut data = Vec::with_capacity(overlap.len() + chunk.len());
            data.extend_from_slice(&overlap);
            data.extend_from_slice(&chunk);

            let data_base = start + offset - overlap.len();
            if data.len() >= pattern.len() {
                for index in 0..=data.len() - pattern.len() {
                    if pattern_matches(&data[index..index + pattern.len()], pattern) {
                        matches.push(data_base + index);
                    }
                }
            }

            overlap.clear();
            let keep = overlap_len.min(data.len());
            overlap.extend_from_slice(&data[data.len() - keep..]);
            offset += read_len;
        }

        matches.sort_unstable();
        matches.dedup();
        Ok(matches)
    }

    pub fn read_u64(&self, address: usize) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.read_exact::<8>(address)?))
    }

    pub fn read_i32(&self, address: usize) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.read_exact::<4>(address)?))
    }

    pub fn read_utf8_z(&self, address: usize, max_len: usize) -> io::Result<String> {
        let bytes = self.read_bytes(address, max_len)?;
        let end = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(bytes.len());
        Ok(String::from_utf8_lossy(&bytes[..end]).to_string())
    }

    pub fn read_utf16_len(&self, address: usize, len: usize) -> io::Result<String> {
        let byte_len = len
            .checked_mul(2)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "UTF-16 length overflow"))?;
        let bytes = self.read_bytes(address, byte_len)?;
        let units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        Ok(String::from_utf16_lossy(&units))
    }

    pub fn read_bytes(&self, address: usize, len: usize) -> io::Result<Vec<u8>> {
        let mut buffer = vec![0u8; len];
        self.read_into(address, &mut buffer)?;
        Ok(buffer)
    }

    fn pe_header_offset(&self) -> io::Result<usize> {
        let base = self.main_module.base_address;
        let dos_header = self.read_exact::<64>(base)?;
        if &dos_header[0..2] != b"MZ" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "main module does not start with an MZ header",
            ));
        }

        let pe_offset = i32::from_le_bytes(
            dos_header[0x3c..0x40]
                .try_into()
                .expect("DOS header slice has fixed length"),
        );
        if !(0x40..=0x10_0000).contains(&pe_offset) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("implausible PE header offset: 0x{pe_offset:x}"),
            ));
        }

        Ok(pe_offset as usize)
    }

    fn read_exact<const N: usize>(&self, address: usize) -> io::Result<[u8; N]> {
        let mut buffer = [0u8; N];
        self.read_into(address, &mut buffer)?;
        Ok(buffer)
    }

    fn read_into(&self, address: usize, buffer: &mut [u8]) -> io::Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }

        let mut bytes_read = 0usize;
        let ok = unsafe {
            ReadProcessMemory(
                self.handle.raw(),
                address as *const c_void,
                buffer.as_mut_ptr().cast::<c_void>(),
                buffer.len(),
                &mut bytes_read,
            )
        };

        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if bytes_read != buffer.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "requested {} bytes at 0x{address:x}, read {bytes_read}",
                    buffer.len()
                ),
            ));
        }

        Ok(())
    }
}

fn pattern_matches(bytes: &[u8], pattern: &[Option<u8>]) -> bool {
    bytes
        .iter()
        .zip(pattern)
        .all(|(byte, expected)| expected.is_none_or(|expected| *byte == expected))
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn new(handle: HANDLE) -> io::Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }

    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

fn open_read_only(pid: u32) -> io::Result<OwnedHandle> {
    let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ;
    let handle = unsafe { OpenProcess(access, 0, pid) };
    OwnedHandle::new(handle)
}

fn find_process_id(target_names: &[&str]) -> io::Result<u32> {
    let snapshot = OwnedHandle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })?;

    let mut entry: PROCESSENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;

    let first_ok = unsafe { Process32FirstW(snapshot.raw(), &mut entry) };
    if first_ok == 0 {
        return Err(io::Error::last_os_error());
    }

    loop {
        let name = utf16_z_to_string(&entry.szExeFile);
        if target_names
            .iter()
            .any(|target| name.eq_ignore_ascii_case(target))
        {
            return Ok(entry.th32ProcessID);
        }

        let next_ok = unsafe { Process32NextW(snapshot.raw(), &mut entry) };
        if next_ok == 0 {
            break;
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Path of Exile 2 process was not found",
    ))
}

fn find_main_module(pid: u32, target_names: &[&str]) -> io::Result<ModuleInfo> {
    let flags = TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32;
    let snapshot = OwnedHandle::new(unsafe { CreateToolhelp32Snapshot(flags, pid) })?;

    let mut entry: MODULEENTRY32W = unsafe { zeroed() };
    entry.dwSize = size_of::<MODULEENTRY32W>() as u32;

    let first_ok = unsafe { Module32FirstW(snapshot.raw(), &mut entry) };
    if first_ok == 0 {
        return Err(io::Error::last_os_error());
    }

    let mut first_module: Option<ModuleInfo> = None;

    loop {
        let module = ModuleInfo {
            name: utf16_z_to_string(&entry.szModule),
            base_address: entry.modBaseAddr as usize,
            size: entry.modBaseSize,
        };

        if first_module.is_none() {
            first_module = Some(module.clone());
        }

        if target_names
            .iter()
            .any(|target| module.name.eq_ignore_ascii_case(target))
        {
            return Ok(module);
        }

        let next_ok = unsafe { Module32NextW(snapshot.raw(), &mut entry) };
        if next_ok == 0 {
            break;
        }
    }

    first_module.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no modules were returned for the PoE2 process",
        )
    })
}

fn utf16_z_to_string(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&ch| ch == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}
