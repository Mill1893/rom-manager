//! Filesystem confinement under mutation (issue #43).
//!
//! Every path beneath a Media Target root is resolved **one validated segment at
//! a time against a retained directory handle**, refusing to traverse any
//! indirection. All I/O then addresses the opened object rather than re-resolving
//! a pathname, so a later step cannot select a different object than the one
//! that was checked.
//!
//! Probe evidence in issue #52 is why this exists rather than a path-string
//! check: an ordinary relative open followed a junction straight out of the
//! managed root, and only the no-reparse flag refused it.
//!
//! # Threat model
//!
//! This defends against **benign concurrent mutation** — the user, a file
//! manager, an indexer, or a backup agent touching the target mid-sync — and
//! against accidental indirection left inside a managed root. It makes **no
//! claim of resistance to a hostile same-privilege process**, which can still
//! win whatever check-to-use window remains. Hard links are a separate aliasing
//! concern that reparse rejection does not address.
//!
//! Never used as confinement: `canonicalize`, `Path::starts_with`,
//! `symlink_metadata`, or any check-then-open sequence on a path string. Each
//! re-resolves a name and is a different operation from the one that follows it.

use std::{io, path::Path};

use crate::RelativePath;

/// A Media Target root, held open for the life of an operation.
pub struct ConfinedRoot(imp::Dir);

impl ConfinedRoot {
    /// Opens `root` as a directory, refusing to follow it if it is itself an
    /// indirection.
    pub fn open(root: &Path) -> io::Result<Self> {
        imp::Dir::open_root(root).map(Self)
    }

    /// Reads the file at `path`, resolving every segment without following
    /// indirection.
    pub fn read(&self, path: &RelativePath) -> io::Result<Vec<u8>> {
        let (parent, name) = self.walk(path)?;
        parent.read_file(&name)
    }

    /// Creates the file at `path` and writes `bytes`, failing if anything
    /// already resolves to that name.
    ///
    /// Creation is atomic create-if-absent — that is the proof the path was
    /// free, and the reason a lexical collision key is only ever a planning
    /// heuristic.
    pub fn write_new(&self, path: &RelativePath, bytes: &[u8]) -> io::Result<()> {
        let (parent, name) = self.walk_creating(path)?;
        parent.create_new(&name, bytes)
    }

    /// Deletes the file at `path`. Bounded to a leaf: this never removes a
    /// directory and never recurses.
    pub fn delete_leaf(&self, path: &RelativePath) -> io::Result<()> {
        let (parent, name) = self.walk(path)?;
        parent.unlink(&name)
    }

    /// Number of names referring to the file at `path`.
    ///
    /// A count above one means the bytes are reachable from somewhere this
    /// application cannot see, so writing through the name would modify content
    /// outside the managed root.
    pub fn link_count(&self, path: &RelativePath) -> io::Result<u64> {
        let (parent, name) = self.walk(path)?;
        parent.link_count(&name)
    }

    /// Resolves every segment but the last, returning the parent directory
    /// handle and the leaf name.
    fn walk(&self, path: &RelativePath) -> io::Result<(imp::Dir, String)> {
        self.walk_inner(path, false)
    }

    /// As [`walk`](Self::walk), creating missing intermediate directories. Each
    /// created directory is then re-opened under the same no-follow rule, so a
    /// directory that appears between the create and the open is not trusted.
    fn walk_creating(&self, path: &RelativePath) -> io::Result<(imp::Dir, String)> {
        self.walk_inner(path, true)
    }

    fn walk_inner(&self, path: &RelativePath, create: bool) -> io::Result<(imp::Dir, String)> {
        let mut segments = path.as_str().split('/').collect::<Vec<_>>();
        let name = segments
            .pop()
            .expect("a validated relative path has at least one segment")
            .to_owned();

        let mut current = self.0.reopen()?;
        for segment in segments {
            current = match current.open_dir(segment) {
                Ok(directory) => directory,
                Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                    current.create_dir(segment)?;
                    current.open_dir(segment)?
                }
                Err(error) => return Err(error),
            };
        }
        Ok((current, name))
    }
}

#[cfg(unix)]
mod imp {
    use std::{
        ffi::CString,
        io::{self, Read, Write},
        os::fd::{AsRawFd, FromRawFd, OwnedFd},
        path::Path,
    };

    /// `O_NOFOLLOW` is the Unix analogue of the Windows no-reparse flag: applied
    /// to every segment, it refuses a symbolic link wherever it appears rather
    /// than only at the leaf.
    pub struct Dir(OwnedFd);

    fn cstr(value: &str) -> io::Result<CString> {
        CString::new(value).map_err(|_| io::Error::other("path contains an interior NUL"))
    }

    fn check(result: libc::c_int) -> io::Result<libc::c_int> {
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result)
        }
    }

    impl Dir {
        pub fn open_root(root: &Path) -> io::Result<Self> {
            let path = cstr(&root.to_string_lossy())?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let fd = check(unsafe { libc::open(path.as_ptr(), flags) })?;
            Ok(Self(unsafe { OwnedFd::from_raw_fd(fd) }))
        }

        pub fn reopen(&self) -> io::Result<Self> {
            self.0.try_clone().map(Self)
        }

        pub fn open_dir(&self, name: &str) -> io::Result<Self> {
            let name = cstr(name)?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let fd = check(unsafe { libc::openat(self.0.as_raw_fd(), name.as_ptr(), flags) })?;
            Ok(Self(unsafe { OwnedFd::from_raw_fd(fd) }))
        }

        pub fn create_dir(&self, name: &str) -> io::Result<()> {
            let name = cstr(name)?;
            check(unsafe { libc::mkdirat(self.0.as_raw_fd(), name.as_ptr(), 0o755) })?;
            Ok(())
        }

        pub fn read_file(&self, name: &str) -> io::Result<Vec<u8>> {
            let name = cstr(name)?;
            let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let fd = check(unsafe { libc::openat(self.0.as_raw_fd(), name.as_ptr(), flags) })?;
            let mut file = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) });
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(bytes)
        }

        pub fn create_new(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
            let name = cstr(name)?;
            // O_EXCL is the proof the name was free; O_NOFOLLOW means an
            // existing symlink at the name is a failure, never a redirect.
            let flags =
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let fd =
                check(unsafe { libc::openat(self.0.as_raw_fd(), name.as_ptr(), flags, 0o644) })?;
            let mut file = std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) });
            file.write_all(bytes)?;
            file.sync_all()
        }

        pub fn unlink(&self, name: &str) -> io::Result<()> {
            let name = cstr(name)?;
            // No AT_REMOVEDIR: a removal is bounded to a leaf file and never
            // takes a directory tree with it.
            check(unsafe { libc::unlinkat(self.0.as_raw_fd(), name.as_ptr(), 0) })?;
            Ok(())
        }

        pub fn link_count(&self, name: &str) -> io::Result<u64> {
            let name = cstr(name)?;
            let mut status: libc::stat = unsafe { std::mem::zeroed() };
            check(unsafe {
                libc::fstatat(
                    self.0.as_raw_fd(),
                    name.as_ptr(),
                    &mut status,
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            })?;
            Ok(status.st_nlink as u64)
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::{
        io::{self, Read, Write},
        os::windows::io::{FromRawHandle, OwnedHandle},
        path::Path,
    };

    use windows_sys::{
        Wdk::{
            Foundation::OBJECT_ATTRIBUTES,
            Storage::FileSystem::{
                FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
                FILE_SYNCHRONOUS_IO_NONALERT,
            },
        },
        Win32::{
            Foundation::{
                HANDLE, NTSTATUS, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, STATUS_SUCCESS,
                UNICODE_STRING,
            },
            Storage::FileSystem::{
                FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
                FILE_SHARE_WRITE,
            },
            System::IO::IO_STATUS_BLOCK,
        },
    };

    // windows-sys 0.61 ships the types and constants but no Nt*File entry
    // points, so the syscalls are declared here against ntdll.
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

    /// Directory handles deliberately omit `FILE_SHARE_DELETE`, so another
    /// process cannot rename or delete a traversed directory out from under an
    /// in-flight sync. The resulting sharing violation is a normal fail-closed
    /// outcome.
    const DIRECTORY_SHARE: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE;

    pub struct Dir(OwnedHandle);

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().collect()
    }

    /// Converts a Win32 path into an NT object-manager path.
    ///
    /// `std::fs::canonicalize` returns an **extended-length** path on Windows —
    /// `\\?\C:\…` — and blindly prefixing `\??\` to that yields
    /// `\??\\\?\C:\…`, which the object manager rejects with
    /// `STATUS_OBJECT_NAME_INVALID`. The two prefixes mean the same thing in
    /// different namespaces, so an existing one is replaced rather than stacked.
    fn nt_object_path(path: &Path) -> String {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            // \\?\UNC\server\share -> \??\UNC\server\share
            return format!(r"\??\UNC\{rest}");
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return format!(r"\??\{rest}");
        }
        if let Some(rest) = text.strip_prefix(r"\\") {
            // A plain UNC path \\server\share.
            return format!(r"\??\UNC\{rest}");
        }
        format!(r"\??\{text}")
    }

    fn open_relative(
        root: Option<&OwnedHandle>,
        name: &mut [u16],
        access: u32,
        share: u32,
        disposition: u32,
        options: u32,
    ) -> io::Result<OwnedHandle> {
        use std::os::windows::io::AsRawHandle;

        let byte_len = (name.len() * 2) as u16;
        let object_name = UNICODE_STRING {
            Length: byte_len,
            MaximumLength: byte_len,
            Buffer: name.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: root.map_or(std::ptr::null_mut(), |handle| {
                handle.as_raw_handle() as HANDLE
            }),
            ObjectName: &object_name,
            // Refuses every reparse tag at every path position — not just at the
            // leaf, and without interpreting any reparse payload.
            Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };

        let mut handle: HANDLE = std::ptr::null_mut();
        let mut iosb: IO_STATUS_BLOCK = unsafe { core::mem::zeroed() };
        let status = unsafe {
            NtCreateFile(
                &mut handle,
                access,
                &attributes,
                &mut iosb,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                share,
                disposition,
                options | FILE_SYNCHRONOUS_IO_NONALERT,
                std::ptr::null(),
                0,
            )
        };
        if status != STATUS_SUCCESS {
            return Err(ntstatus_error(status));
        }
        Ok(unsafe { OwnedHandle::from_raw_handle(handle as _) })
    }

    /// Maps an NTSTATUS onto an `io::Error` **with its kind preserved**.
    ///
    /// Stringifying the status loses the kind, and callers genuinely branch on
    /// it: the confined walk creates a missing directory only when an open
    /// reports `NotFound`, so a status flattened to `Other` silently turned
    /// "create the parent" into "fail". CI caught exactly that.
    fn ntstatus_error(status: NTSTATUS) -> io::Error {
        const STATUS_OBJECT_NAME_NOT_FOUND: NTSTATUS = 0xC000_0034_u32 as NTSTATUS;
        const STATUS_OBJECT_PATH_NOT_FOUND: NTSTATUS = 0xC000_003A_u32 as NTSTATUS;
        const STATUS_OBJECT_NAME_COLLISION: NTSTATUS = 0xC000_0035_u32 as NTSTATUS;
        const STATUS_ACCESS_DENIED: NTSTATUS = 0xC000_0022_u32 as NTSTATUS;
        const STATUS_SHARING_VIOLATION: NTSTATUS = 0xC000_0043_u32 as NTSTATUS;
        const STATUS_REPARSE_POINT_ENCOUNTERED: NTSTATUS = 0xC000_050B_u32 as NTSTATUS;

        let kind = match status {
            STATUS_OBJECT_NAME_NOT_FOUND | STATUS_OBJECT_PATH_NOT_FOUND => io::ErrorKind::NotFound,
            STATUS_OBJECT_NAME_COLLISION => io::ErrorKind::AlreadyExists,
            STATUS_ACCESS_DENIED => io::ErrorKind::PermissionDenied,
            // A reparse point refused mid-walk is the confinement guarantee
            // firing, not a missing file — it must never look creatable.
            STATUS_SHARING_VIOLATION | STATUS_REPARSE_POINT_ENCOUNTERED => io::ErrorKind::Other,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, format!("NTSTATUS 0x{:08X}", status as u32))
    }

    impl Dir {
        pub fn open_root(root: &Path) -> io::Result<Self> {
            // The root is reached by a fully qualified name, then inspected: if
            // the root is itself a reparse point, OBJ_DONT_REPARSE refuses it.
            let mut name = wide(&nt_object_path(root));
            open_relative(
                None,
                &mut name,
                FILE_GENERIC_READ,
                DIRECTORY_SHARE,
                FILE_OPEN,
                FILE_DIRECTORY_FILE,
            )
            .map(Self)
        }

        pub fn reopen(&self) -> io::Result<Self> {
            self.0.try_clone().map(Self)
        }

        pub fn open_dir(&self, name: &str) -> io::Result<Self> {
            let mut name = wide(name);
            open_relative(
                Some(&self.0),
                &mut name,
                FILE_GENERIC_READ,
                DIRECTORY_SHARE,
                FILE_OPEN,
                FILE_DIRECTORY_FILE,
            )
            .map(Self)
        }

        pub fn create_dir(&self, name: &str) -> io::Result<()> {
            let mut name = wide(name);
            open_relative(
                Some(&self.0),
                &mut name,
                FILE_GENERIC_WRITE,
                DIRECTORY_SHARE,
                FILE_CREATE,
                FILE_DIRECTORY_FILE,
            )
            .map(|_| ())
        }

        pub fn read_file(&self, name: &str) -> io::Result<Vec<u8>> {
            let mut name = wide(name);
            let handle = open_relative(
                Some(&self.0),
                &mut name,
                FILE_GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                FILE_OPEN,
                FILE_NON_DIRECTORY_FILE,
            )?;
            let mut file = std::fs::File::from(handle);
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            Ok(bytes)
        }

        pub fn create_new(&self, name: &str, bytes: &[u8]) -> io::Result<()> {
            let mut name = wide(name);
            // FILE_CREATE is the atomic create-if-absent that proves the name
            // was free; an existing object fails the action.
            let handle = open_relative(
                Some(&self.0),
                &mut name,
                FILE_GENERIC_WRITE,
                FILE_SHARE_READ,
                FILE_CREATE,
                FILE_NON_DIRECTORY_FILE,
            )?;
            let mut file = std::fs::File::from(handle);
            file.write_all(bytes)?;
            file.sync_all()
        }

        pub fn unlink(&self, name: &str) -> io::Result<()> {
            // Opened with no-reparse and deleted through *that* handle, so a
            // later pathname lookup cannot select a different object.
            let mut wide_name = wide(name);
            let handle = open_relative(
                Some(&self.0),
                &mut wide_name,
                FILE_GENERIC_READ | 0x0001_0000, // DELETE
                FILE_SHARE_READ,
                FILE_OPEN,
                FILE_NON_DIRECTORY_FILE,
            )?;
            let file = std::fs::File::from(handle);
            // Handle-based disposition; never a pathname delete.
            crate::confined::windows_delete_on_close(&file)
        }

        pub fn link_count(&self, name: &str) -> io::Result<u64> {
            let mut name = wide(name);
            let handle = open_relative(
                Some(&self.0),
                &mut name,
                FILE_GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                FILE_OPEN,
                FILE_NON_DIRECTORY_FILE,
            )?;
            let file = std::fs::File::from(handle);
            super::link_count_by_handle(&file)
        }
    }
}

/// `std::os::windows::fs::MetadataExt::number_of_links` is still unstable, so
/// the count is read through the documented handle API.
#[cfg(windows)]
fn link_count_by_handle(file: &std::fs::File) -> io::Result<u64> {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };

    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { core::mem::zeroed() };
    let ok =
        unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut information) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(information.nNumberOfLinks as u64)
}

#[cfg(windows)]
fn windows_delete_on_close(file: &std::fs::File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
        },
    };

    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
