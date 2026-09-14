param(
  [Parameter(Mandatory = $true)][string]$Root,
  [Parameter(Mandatory = $true)][string]$ExpectedExecutable
)

$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path $Root).Path
$libraryDirectory = Join-Path $Root 'lib\jellypilot'
$share = Join-Path $Root 'share\jellypilot'
$baseline = Join-Path $share 'mpv-baseline.conf'
$runtime = Get-Content (Join-Path $share 'windows-runtime.json') -Raw | ConvertFrom-Json
$manifest = Get-Content (Join-Path $share 'mpv-manifest.json') -Raw | ConvertFrom-Json
if ((Get-FileHash (Join-Path $Root 'jellypilot.exe')).Hash -ne (Get-FileHash $ExpectedExecutable).Hash) {
  throw 'Packaged launcher differs from the release executable'
}
foreach ($file in $runtime.bundled) {
  if ((Get-FileHash (Join-Path $libraryDirectory $file.name)).Hash -ne $file.sha256) {
    throw "Packaged DLL hash mismatch: $($file.name)"
  }
}
foreach ($file in $runtime.launcher.bundled) {
  if ((Get-FileHash (Join-Path $Root $file.name)).Hash -ne $file.sha256) {
    throw "Packaged launcher runtime hash mismatch: $($file.name)"
  }
}
foreach ($artifact in $manifest.artifacts.PSObject.Properties) {
  if ((Get-FileHash (Join-Path $Root $artifact.Name)).Hash -ne $artifact.Value.sha256) {
    throw "Packaged mpv artifact hash mismatch: $($artifact.Name)"
  }
}
if (!(Select-String -Path $baseline -Pattern '^hwdec=d3d11va-copy$' -Quiet)) {
  throw 'Packaged Windows baseline does not select d3d11va-copy'
}
if (@(Get-ChildItem (Join-Path $share 'licenses\msys2') -Recurse -File).Count -eq 0) {
  throw 'Missing dependency license notices'
}

# Load using the host's exact flags, without a developer PATH or resource overrides.
# This is a headless DLL/configuration gate; it does not claim GPU playback or a GUI frame.
$env:JELLYPILOT_LIBMPV = $null
$env:JELLYPILOT_MPV_BASELINE = $null
$env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;

public static class PackagedMpvProbe {
  [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)]
  static extern IntPtr LoadLibraryExW(string path, IntPtr file, uint flags);
  [DllImport("kernel32", CharSet = CharSet.Ansi, ExactSpelling = true)]
  static extern IntPtr GetProcAddress(IntPtr library, string name);
  [DllImport("kernel32")]
  static extern bool FreeLibrary(IntPtr library);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate IntPtr Create();
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate int Initialize(IntPtr handle);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate void Destroy(IntPtr handle);
  [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
  delegate int SetOption(IntPtr handle, [MarshalAs(UnmanagedType.LPUTF8Str)] string name,
    [MarshalAs(UnmanagedType.LPUTF8Str)] string value);

  static IntPtr Symbol(IntPtr library, string name) {
    IntPtr symbol = GetProcAddress(library, name);
    if (symbol == IntPtr.Zero) throw new Exception("Missing pinned host symbol: " + name);
    return symbol;
  }

  public static void Run(string dll, string baseline) {
    IntPtr library = LoadLibraryExW(dll, IntPtr.Zero, 0x1100);
    if (library == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
    try {
      Symbol(library, "mpv_gpu_next_set_host");
      Symbol(library, "mpv_gpu_next_request_redraw");
      var create = (Create)Marshal.GetDelegateForFunctionPointer(Symbol(library, "mpv_create"), typeof(Create));
      var destroy = (Destroy)Marshal.GetDelegateForFunctionPointer(Symbol(library, "mpv_terminate_destroy"), typeof(Destroy));
      var initialize = (Initialize)Marshal.GetDelegateForFunctionPointer(Symbol(library, "mpv_initialize"), typeof(Initialize));
      var option = (SetOption)Marshal.GetDelegateForFunctionPointer(Symbol(library, "mpv_set_option_string"), typeof(SetOption));
      IntPtr handle = create();
      if (handle == IntPtr.Zero) throw new Exception("mpv_create failed");
      try {
        if (option(handle, "config", "no") < 0) throw new Exception("config=no failed");
        foreach (string raw in File.ReadAllLines(baseline)) {
          string line = raw.Trim();
          if (line.Length == 0 || line.StartsWith("#")) continue;
          int split = line.IndexOf('=');
          if (split <= 0 || option(handle, line.Substring(0, split), line.Substring(split + 1)) < 0)
            throw new Exception("Packaged baseline option failed: " + line);
        }
        if (option(handle, "terminal", "no") < 0) throw new Exception("terminal=no failed");
        int result = initialize(handle);
        if (result < 0) throw new Exception("mpv_initialize failed: " + result);
      } finally { destroy(handle); }
    } finally { FreeLibrary(library); }
  }
}
'@
[PackagedMpvProbe]::Run((Join-Path $libraryDirectory 'libmpv-2.dll'), $baseline)
[PSCustomObject]@{
  root = $Root
  bundledDlls = @($runtime.bundled).Count
  hashesVerified = $true
  dllInitialized = $true
  baseline = 'd3d11va-copy'
  gpuPlayback = 'not tested by this headless gate'
} | ConvertTo-Json
