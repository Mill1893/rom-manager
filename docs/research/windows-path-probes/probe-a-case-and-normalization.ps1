$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class W {
  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr CreateFileW(string path, uint access, uint share,
    IntPtr sa, uint disp, uint flags, IntPtr tmpl);

  [DllImport("kernel32.dll", SetLastError=true)]
  public static extern bool CloseHandle(IntPtr h);

  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern uint GetShortPathNameW(string lpsz, StringBuilder buf, uint cch);

  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern uint GetLongPathNameW(string lpsz, StringBuilder buf, uint cch);

  [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool CreateDirectoryW(string path, IntPtr sa);

  public const uint GENERIC_READ  = 0x80000000;
  public const uint GENERIC_WRITE = 0x40000000;
  public const uint CREATE_NEW    = 1;
  public const uint OPEN_EXISTING = 3;
  public const uint FLAG_BACKUP   = 0x02000000;
  public const uint FLAG_REPARSE  = 0x00200000;
}
'@

function New-File([string]$path) {
  $h = [W]::CreateFileW($path, [W]::GENERIC_WRITE, 0, [IntPtr]::Zero, [W]::CREATE_NEW, 0, [IntPtr]::Zero)
  if ($h -eq [IntPtr](-1)) { return @{ ok=$false; err=[Runtime.InteropServices.Marshal]::GetLastWin32Error() } }
  [void][W]::CloseHandle($h); return @{ ok=$true; err=0 }
}

function Open-File([string]$path) {
  $h = [W]::CreateFileW($path, [W]::GENERIC_READ, 3, [IntPtr]::Zero, [W]::OPEN_EXISTING, 0, [IntPtr]::Zero)
  if ($h -eq [IntPtr](-1)) { return @{ ok=$false; err=[Runtime.InteropServices.Marshal]::GetLastWin32Error() } }
  [void][W]::CloseHandle($h); return @{ ok=$true; err=0 }
}

function Say($k, $v) { Write-Output ("{0}={1}" -f $k, $v) }

# ---------------- environment ----------------
$root = Join-Path $env:TEMP 'rm-probe'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
[void](New-Item -ItemType Directory -Path $root)

Say 'PROBE_ROOT' $root
Say 'OS' ([System.Environment]::OSVersion.VersionString)
$vol = (Get-Item $root).PSDrive.Name + ':'
Say 'VOLUME' $vol
$fsinfo = (Get-Volume -DriveLetter $vol.Substring(0,1))
Say 'FILESYSTEM' $fsinfo.FileSystemType
$id = [Security.Principal.WindowsIdentity]::GetCurrent()
$pr = New-Object Security.Principal.WindowsPrincipal($id)
Say 'IS_ADMIN' $pr.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
Say '8DOT3_VOLUME_QUERY' ((fsutil 8dot3name query $vol 2>&1) -join ' | ')

Write-Output '--- P1 case-insensitive lookup ---'
# baseline ASCII
[void](New-File (Join-Path $root 'Tracers.nes'))
Say 'P1_ascii_lower' (Open-File (Join-Path $root 'tracers.nes')).ok
Say 'P1_ascii_upper' (Open-File (Join-Path $root 'TRACERS.NES')).ok

# Each pair: create name A, then try to open name B. ok=$true means OS folded them together.
$pairs = @(
  @{ n='turkish_dotless_i'; a=[string][char]0x0131;                 b='I' },
  @{ n='turkish_dotted_I';  a=[string][char]0x0130;                 b='i' },
  @{ n='kelvin_sign';       a=[string][char]0x212A;                 b='K' },
  @{ n='angstrom_sign';     a=[string][char]0x212B;                 b=[string][char]0x00C5 },
  @{ n='sharp_s';           a=[string][char]0x00DF;                 b='SS' },
  @{ n='final_sigma';       a=[string][char]0x03C2;                 b=[string][char]0x03C3 },
  @{ n='greek_sigma_case';  a=[string][char]0x03C3;                 b=[string][char]0x03A3 },
  @{ n='cyrillic_a_vs_lat'; a=[string][char]0x0430;                 b='a' },
  @{ n='latin_e_acute';     a=[string][char]0x00E9;                 b=[string][char]0x00C9 },
  @{ n='fullwidth_a';       a=[string][char]0xFF41;                 b=[string][char]0xFF21 },
  @{ n='ligature_ff';       a=[string][char]0xFB00;                 b='ff' },
  @{ n='deseret_long_i';    a=[char]::ConvertFromUtf32(0x10428);    b=[char]::ConvertFromUtf32(0x10400) }
)
foreach ($p in $pairs) {
  $d = Join-Path $root ('p1_' + $p.n)
  [void][W]::CreateDirectoryW($d, [IntPtr]::Zero)
  $mk = New-File (Join-Path $d ($p.a + '.nes'))
  if (-not $mk.ok) { Say ('P1_' + $p.n) ('CREATE_FAILED_' + $mk.err); continue }
  $op = Open-File (Join-Path $d ($p.b + '.nes'))
  # also: can both coexist as separate files?
  $co = New-File (Join-Path $d ($p.b + '.nes'))
  Say ('P1_' + $p.n) ('folded=' + $op.ok + ' coexist=' + $co.ok)
}

Write-Output '--- P3 unicode normalization ---'
$d3 = Join-Path $root 'p3_norm'
[void][W]::CreateDirectoryW($d3, [IntPtr]::Zero)
$nfc = [string][char]0x00E9                      # e-acute precomposed
$nfd = 'e' + [string][char]0x0301                # e + combining acute
[void](New-File (Join-Path $d3 ($nfc + '.nes')))
Say 'P3_open_NFD_after_NFC' (Open-File (Join-Path $d3 ($nfd + '.nes'))).ok
Say 'P3_coexist_NFD'        (New-File  (Join-Path $d3 ($nfd + '.nes'))).ok
$names = [System.IO.Directory]::GetFiles($d3) | ForEach-Object {
  $n = [System.IO.Path]::GetFileNameWithoutExtension($_)
  ($n.ToCharArray() | ForEach-Object { 'U+{0:X4}' -f [int]$_ }) -join ','
}
Say 'P3_stored_names' ($names -join ' ; ')
