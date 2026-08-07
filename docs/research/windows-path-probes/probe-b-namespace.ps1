$ErrorActionPreference = 'Continue'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class W2 {
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr CreateFileW(string path, uint access, uint share,
    IntPtr sa, uint disp, uint flags, IntPtr tmpl);
  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern uint GetShortPathNameW(string lpsz, StringBuilder buf, uint cch);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool CreateDirectoryW(string path, IntPtr sa);
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern uint GetFullPathNameW(string lpsz, uint cch, StringBuilder buf, IntPtr fp);
}
'@
function New-File2([string]$p) {
  $h = [W2]::CreateFileW($p, 0x40000000, 0, [IntPtr]::Zero, 1, 0, [IntPtr]::Zero)
  if ($h -eq [IntPtr](-1)) { return @{ ok=$false; err=[Runtime.InteropServices.Marshal]::GetLastWin32Error() } }
  [void][W2]::CloseHandle($h); return @{ ok=$true; err=0 }
}
function Say($k,$v){ Write-Output ("{0}={1}" -f $k,$v) }

$root = Join-Path $env:TEMP 'rm-probe'
$rootNt = '\\?\' + $root

Write-Output '--- P4 trailing dots and spaces ---'
$d4 = Join-Path $root 'p4_trailing'; [void][W2]::CreateDirectoryW($d4,[IntPtr]::Zero)
$d4nt = '\\?\' + $d4
foreach ($case in @(@{n='trailing_dot';v='rom.nes.'}, @{n='trailing_space';v='rom.nes '}, @{n='trailing_dotspace';v='rom.nes. '})) {
  $r1 = New-File2 (Join-Path $d4 $case.v)
  $r2 = New-File2 ($d4nt + '\' + $case.v)
  # what did the Win32 path actually resolve to?
  $sb = New-Object Text.StringBuilder 1024
  [void][W2]::GetFullPathNameW((Join-Path $d4 $case.v), 1024, $sb, [IntPtr]::Zero)
  Say ('P4_' + $case.n) ('win32_create=' + $r1.ok + '(' + $r1.err + ') verbatim_create=' + $r2.ok + '(' + $r2.err + ')')
  Say ('P4_' + $case.n + '_fullpath') ("'" + $sb.ToString() + "'")
}
$listed = [System.IO.Directory]::GetFiles($d4) | ForEach-Object { "'" + [System.IO.Path]::GetFileName($_) + "'" }
Say 'P4_listed_win32' ($listed -join ' ; ')
$listedNt = [System.IO.Directory]::GetFiles($d4nt) | ForEach-Object { "'" + [System.IO.Path]::GetFileName($_) + "'" }
Say 'P4_listed_verbatim' ($listedNt -join ' ; ')

Write-Output '--- P5 reserved device names ---'
$d5 = Join-Path $root 'p5_reserved'; [void][W2]::CreateDirectoryW($d5,[IntPtr]::Zero)
$d5nt = '\\?\' + $d5
foreach ($n in @('CON','NUL','AUX','PRN','COM1','LPT1','CON.nes','nul.nes','COM9.nes','COM0.nes','CONOUT$','clock$')) {
  $r1 = New-File2 (Join-Path $d5 $n)
  $r2 = New-File2 ($d5nt + '\' + $n)
  Say ('P5_' + ($n -replace '\$','S')) ('win32=' + $r1.ok + '(' + $r1.err + ') verbatim=' + $r2.ok + '(' + $r2.err + ')')
}
$l5 = [System.IO.Directory]::GetFiles($d5nt) | ForEach-Object { [System.IO.Path]::GetFileName($_) }
Say 'P5_actually_created' ($l5 -join ' ; ')

Write-Output '--- P6 short name (8.3) aliases ---'
$d6 = Join-Path $root 'p6_shortname'; [void][W2]::CreateDirectoryW($d6,[IntPtr]::Zero)
[void](New-File2 (Join-Path $d6 'Super Long Tracer Name.nes'))
[void](New-File2 (Join-Path $d6 'Super Long Tracer Name 2.nes'))
$sb = New-Object Text.StringBuilder 1024
$n = [W2]::GetShortPathNameW((Join-Path $d6 'Super Long Tracer Name.nes'), $sb, 1024)
Say 'P6_getshortpathname_len' $n
Say 'P6_short_of_long' ("'" + $sb.ToString() + "'")
Say 'P6_dir_x' (((cmd.exe /c "dir /x `"$d6`"" 2>&1) | Select-String -Pattern 'nes') -join ' || ')
$reg = 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem'
Say 'P6_NtfsDisable8dot3' (Get-ItemProperty -Path $reg -Name NtfsDisable8dot3NameCreation -ErrorAction SilentlyContinue).NtfsDisable8dot3NameCreation

Write-Output '--- P2 per-directory case sensitivity ---'
$d2 = Join-Path $root 'p2_case'; [void][W2]::CreateDirectoryW($d2,[IntPtr]::Zero)
Say 'P2_query_default' ((fsutil file queryCaseSensitiveInfo $d2 2>&1) -join ' | ')
Say 'P2_enable_attempt' ((fsutil file setCaseSensitiveInfo $d2 enable 2>&1) -join ' | ')
Say 'P2_query_after'   ((fsutil file queryCaseSensitiveInfo $d2 2>&1) -join ' | ')
$a = New-File2 (Join-Path $d2 'rom.nes')
$b = New-File2 (Join-Path $d2 'ROM.nes')
Say 'P2_coexist_after_enable' ('a=' + $a.ok + ' b=' + $b.ok + '(' + $b.err + ')')
