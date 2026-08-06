$ErrorActionPreference = 'Continue'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class W3 {
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr CreateFileW(string path, uint access, uint share,
    IntPtr sa, uint disp, uint flags, IntPtr tmpl);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool WriteFile(IntPtr h, byte[] buf, uint n, out uint written, IntPtr ov);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern uint GetFileType(IntPtr h);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool CreateDirectoryW(string path, IntPtr sa);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern uint GetFinalPathNameByHandleW(IntPtr h, StringBuilder buf, uint cch, uint flags);
}
'@
function Say($k,$v){ Write-Output ("{0}={1}" -f $k,$v) }

$root = Join-Path $env:TEMP 'rm-probe2'
if (Test-Path ('\\?\' + $root)) { Remove-Item -Recurse -Force ('\\?\' + $root) }
[void][W3]::CreateDirectoryW($root,[IntPtr]::Zero)

Write-Output '--- P5b reserved names, verified ---'
foreach ($n in @('CON','NUL','PRN','COM1','CON.nes','LPT1.nes')) {
  $d = Join-Path $root ('r_' + ($n -replace '\.','_'))
  [void][W3]::CreateDirectoryW($d,[IntPtr]::Zero)
  $p = Join-Path $d $n
  $h = [W3]::CreateFileW($p, 0x40000000, 0, [IntPtr]::Zero, 1, 0, [IntPtr]::Zero)
  if ($h -eq [IntPtr](-1)) {
    Say ('P5b_' + ($n -replace '\.','_')) ('create_failed err=' + [Runtime.InteropServices.Marshal]::GetLastWin32Error())
    continue
  }
  # 1=DISK 2=CHAR 3=PIPE
  $ft = [W3]::GetFileType($h)
  $sb = New-Object Text.StringBuilder 1024
  [void][W3]::GetFinalPathNameByHandleW($h, $sb, 1024, 0)
  $w = 0
  [void][W3]::WriteFile($h, [byte[]](1,2,3,4), 4, [ref]$w, [IntPtr]::Zero)
  [void][W3]::CloseHandle($h)
  # what is actually on disk, seen through the verbatim namespace?
  $ents = @()
  try { $ents = [System.IO.Directory]::GetFileSystemEntries('\\?\' + $d) | ForEach-Object { [System.IO.Path]::GetFileName($_) } } catch { $ents = @('ENUM_ERR') }
  $size = -1
  try { $size = (Get-Item -LiteralPath ('\\?\' + $p) -Force).Length } catch { $size = -1 }
  Say ('P5b_' + ($n -replace '\.','_')) ('filetype=' + $ft + ' wrote=' + $w + ' size_on_disk=' + $size + ' entries=[' + ($ents -join ',') + '] final=' + $sb.ToString())
}
