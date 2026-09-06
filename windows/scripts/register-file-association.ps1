[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Executable,
    [string]$ProfileDirectory,
    [switch]$Unregister
)
$ErrorActionPreference = 'Stop'
Import-Module "$PSScriptRoot/FileAssociation.psm1" -Force
$executablePath = [IO.Path]::GetFullPath($Executable)
if (!$Unregister) {
    foreach ($asset in @('VniDrop.dll', 'App.xbf', 'VniDrop.pri', 'vnidrop_native.dll')) {
        if (!(Test-Path -LiteralPath (Join-Path ([IO.Path]::GetDirectoryName($executablePath)) $asset) -PathType Leaf)) {
            throw "Native app output is missing $asset. Build or publish windows/VniDrop first."
        }
    }
}
$userRoot = [Microsoft.Win32.Registry]::CurrentUser
$machineClasses = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('Software\Classes')
try {
    if ($Unregister) {
        $removed = Unregister-VniDropFileAssociation -UserRoot $userRoot -Executable $executablePath
        if (!$removed) { Write-Output 'This executable does not own the native .vnd registration.'; return }
    } else {
        Register-VniDropFileAssociation -UserRoot $userRoot -Executable $executablePath -ProfileDirectory $ProfileDirectory -MachineClasses $machineClasses
    }
} finally { $userRoot.Dispose(); if ($machineClasses) { $machineClasses.Dispose() } }
if (!('VniDrop.AssociationShell' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace VniDrop {
    public static class AssociationShell {
        [DllImport("shell32.dll")]
        public static extern void SHChangeNotify(uint eventId, uint flags, IntPtr item1, IntPtr item2);
    }
}
'@
}
# Wait for Explorer to receive the invalidation before the registration process exits.
[VniDrop.AssociationShell]::SHChangeNotify(0x08000000, 0x1000, [IntPtr]::Zero, [IntPtr]::Zero)
if ($Unregister) { Write-Output 'Removed the native VniDrop file handler.' }
else { Write-Output 'Registered the native VniDrop file handler. Choose VniDrop in Windows Open with or Default apps to make it the .vnd default.' }
