$ErrorActionPreference = 'Continue'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

[StructLayout(LayoutKind.Sequential)]
public struct UNICODE_STRING { public ushort Length; public ushort MaximumLength; public IntPtr Buffer; }

[StructLayout(LayoutKind.Sequential)]
public struct OBJECT_ATTRIBUTES {
  public int Length; public IntPtr RootDirectory; public IntPtr ObjectName;
  public uint Attributes; public IntPtr SecurityDescriptor; public IntPtr SecurityQualityOfService;
}

[StructLayout(LayoutKind.Sequential)]
public struct IO_STATUS_BLOCK { public IntPtr Status; public IntPtr Information; }

public static class Nt {
  [DllImport("ntdll.dll")]
  public static extern int NtCreateFile(out IntPtr FileHandle, uint DesiredAccess,
    ref OBJECT_ATTRIBUTES ObjectAttributes, out IO_STATUS_BLOCK IoStatusBlock,
    IntPtr AllocationSize, uint FileAttributes, uint ShareAccess, uint CreateDisposition,
    uint CreateOptions, IntPtr EaBuffer, uint EaLength);

  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr CreateFileW(string p, uint a, uint s, IntPtr sa, uint d, uint f, IntPtr t);
  [DllImport("kernel32.dll", SetLastError=true)] public static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode)]
  public static extern uint GetFinalPathNameByHandleW(IntPtr h, StringBuilder b, uint c, uint f);

  public const uint OBJ_CASE_INSENSITIVE = 0x40;
  public const uint OBJ_DONT_REPARSE     = 0x1000;
  public const uint FILE_OPEN            = 1;
  public const uint FILE_OPEN_REPARSE_POINT = 0x00200000;
  public const uint FILE_SYNCHRONOUS_IO_NONALERT = 0x20;
  public const uint SYNCHRONIZE = 0x100000;
  public const uint FILE_GENERIC_READ = 0x120089;

  // Open `name` relative to directory handle `root`.
  public static string OpenRel(IntPtr root, string name, bool dontReparse, bool openReparse) {
    IntPtr buf = Marshal.StringToHGlobalUni(name);
    UNICODE_STRING us = new UNICODE_STRING();
    us.Length = (ushort)(name.Length * 2);
    us.MaximumLength = us.Length;
    us.Buffer = buf;
    IntPtr pus = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(UNICODE_STRING)));
    Marshal.StructureToPtr(us, pus, false);

    OBJECT_ATTRIBUTES oa = new OBJECT_ATTRIBUTES();
    oa.Length = Marshal.SizeOf(typeof(OBJECT_ATTRIBUTES));
    oa.RootDirectory = root;
    oa.ObjectName = pus;
    oa.Attributes = OBJ_CASE_INSENSITIVE | (dontReparse ? OBJ_DONT_REPARSE : 0);

    IntPtr h; IO_STATUS_BLOCK iosb;
    uint opts = FILE_SYNCHRONOUS_IO_NONALERT | (openReparse ? FILE_OPEN_REPARSE_POINT : 0);
    int st = NtCreateFile(out h, FILE_GENERIC_READ | SYNCHRONIZE, ref oa, out iosb,
                          IntPtr.Zero, 0, 7, FILE_OPEN, opts, IntPtr.Zero, 0);
    Marshal.FreeHGlobal(pus); Marshal.FreeHGlobal(buf);
    if (st != 0) return "STATUS=0x" + st.ToString("X8");
    StringBuilder sb = new StringBuilder(1024);
    GetFinalPathNameByHandleW(h, sb, 1024, 0);
    CloseHandle(h);
    return "OK final=" + sb.ToString();
  }
}
'@
function Say($k,$v){ Write-Output ("{0}={1}" -f $k,$v) }

$root = Join-Path $env:TEMP 'rm-probe3'
if (Test-Path $root) { cmd.exe /c "rmdir /s /q `"$root`"" 2>&1 | Out-Null }
[void](New-Item -ItemType Directory -Path $root)
[void](New-Item -ItemType Directory -Path (Join-Path $root 'managed'))
Set-Content -LiteralPath (Join-Path $root 'managed\ok.nes') -Value 'rom' -NoNewline
Set-Content -LiteralPath (Join-Path $root 'outside.txt') -Value 'secret' -NoNewline

# junction (no admin needed) pointing outside the managed root
Say 'SETUP_junction' ((cmd.exe /c "mklink /J `"$root\managed\evil`" `"$root`"" 2>&1) -join ' ')
# symlink attempt (needs admin or Developer Mode)
Say 'SETUP_symlink'  ((cmd.exe /c "mklink `"$root\managed\link.nes`" `"$root\outside.txt`"" 2>&1) -join ' ')

$FILE_FLAG_BACKUP = 0x02000000
$rh = [Nt]::CreateFileW($root, 0x100001, 7, [IntPtr]::Zero, 3, $FILE_FLAG_BACKUP, [IntPtr]::Zero)
Say 'ROOT_HANDLE_OK' ($rh -ne [IntPtr](-1))

Write-Output '--- P7 root-relative opens ---'
Say 'P7_plain_child'          ([Nt]::OpenRel($rh, 'managed\ok.nes', $false, $false))
Say 'P7_case_variant'         ([Nt]::OpenRel($rh, 'MANAGED\OK.NES', $false, $false))
Say 'P7_dotdot_escape'        ([Nt]::OpenRel($rh, 'managed\..\outside.txt', $false, $false))
Say 'P7_leading_backslash'    ([Nt]::OpenRel($rh, '\managed\ok.nes', $false, $false))
Say 'P7_absolute_name'        ([Nt]::OpenRel($rh, 'C:\Windows\notepad.exe', $false, $false))
Say 'P7_junction_traverse'    ([Nt]::OpenRel($rh, 'managed\evil\outside.txt', $false, $false))
Say 'P7_junction_dontreparse' ([Nt]::OpenRel($rh, 'managed\evil\outside.txt', $true, $false))
Say 'P7_junction_itself_open' ([Nt]::OpenRel($rh, 'managed\evil', $false, $true))
Say 'P7_junction_leaf_dontrep'([Nt]::OpenRel($rh, 'managed\evil', $true, $false))
Say 'P7_symlink_traverse'     ([Nt]::OpenRel($rh, 'managed\link.nes', $false, $false))
Say 'P7_symlink_dontreparse'  ([Nt]::OpenRel($rh, 'managed\link.nes', $true, $false))
Say 'P7_trailing_dot_rel'     ([Nt]::OpenRel($rh, 'managed\ok.nes.', $false, $false))
[void][Nt]::CloseHandle($rh)
