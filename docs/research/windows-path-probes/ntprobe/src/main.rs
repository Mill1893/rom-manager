//! Binding probe: proves the root-handle-relative, no-reparse open used by the
//! confinement design can be expressed in Rust. Compilation is the assertion.
#![cfg(windows)]

use std::ptr::{null, null_mut};

use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT},
    },
    Win32::{
        Foundation::{HANDLE, NTSTATUS, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, UNICODE_STRING},
        Storage::FileSystem::{FILE_GENERIC_READ, FILE_SHARE_READ},
        System::IO::IO_STATUS_BLOCK,
    },
};

// windows-sys 0.61 ships the types and constants but no Nt*File entry points,
// so the syscall itself is declared here against ntdll.
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtCreateFile(
        file_handle: *mut HANDLE,
        desired_access: u32,
        object_attributes: *const OBJECT_ATTRIBUTES,
        io_status_block: *mut IO_STATUS_BLOCK,
        allocation_size: *const i64,
        file_attributes: u32,
        share_access: u32,
        create_disposition: u32,
        create_options: u32,
        ea_buffer: *const core::ffi::c_void,
        ea_length: u32,
    ) -> NTSTATUS;
}

/// `STATUS_REPARSE_POINT_ENCOUNTERED` — refused rather than followed.
const STATUS_REPARSE_POINT_ENCOUNTERED: NTSTATUS = 0xC000_050B_u32 as NTSTATUS;

/// Open `name` strictly beneath `root`, refusing to traverse any reparse point.
unsafe fn open_confined(root: HANDLE, name: &mut [u16]) -> Result<HANDLE, NTSTATUS> {
    let byte_len = (name.len() * 2) as u16;
    let object_name = UNICODE_STRING {
        Length: byte_len,
        MaximumLength: byte_len,
        Buffer: name.as_mut_ptr(),
    };

    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: root,
        ObjectName: &object_name,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        SecurityDescriptor: null(),
        SecurityQualityOfService: null(),
    };

    let mut handle: HANDLE = null_mut();
    let mut iosb: IO_STATUS_BLOCK = unsafe { core::mem::zeroed() };

    let status = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_GENERIC_READ,
            &attributes,
            &mut iosb,
            null(),
            0,
            FILE_SHARE_READ,
            FILE_OPEN,
            FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            null(),
            0,
        )
    };

    if status < 0 { Err(status) } else { Ok(handle) }
}

fn main() {
    let mut name: Vec<u16> = "ok.nes".encode_utf16().collect();
    match unsafe { open_confined(null_mut(), &mut name) } {
        Ok(_) => println!("opened"),
        Err(STATUS_REPARSE_POINT_ENCOUNTERED) => println!("refused: reparse point"),
        Err(status) => println!("status=0x{:08X}", status as u32),
    }
}
