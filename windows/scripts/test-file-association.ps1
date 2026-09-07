$ErrorActionPreference = 'Stop'
Import-Module "$PSScriptRoot/FileAssociation.psm1" -Force
if (!('VniDrop.AssociationTestArguments' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace VniDrop {
    public static class AssociationTestArguments {
        [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CommandLineToArgvW(string command, out int count);
        [DllImport("kernel32.dll")]
        private static extern IntPtr LocalFree(IntPtr memory);
        public static string[] Parse(string command) {
            int count;
            var argv = CommandLineToArgvW(command, out count);
            if (argv == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
            try {
                var result = new string[count];
                for (int i = 0; i < count; i++) result[i] = Marshal.PtrToStringUni(Marshal.ReadIntPtr(argv, i * IntPtr.Size));
                return result;
            } finally { LocalFree(argv); }
        }
    }
}
'@
}
function Assert-Equal($Actual, $Expected, [string]$Message) {
    if (($Actual | ConvertTo-Json -Compress) -cne ($Expected | ConvertTo-Json -Compress)) { throw $Message }
}
function Read-Value([string]$Path, [string]$Name = '') {
    $key = $testRoot.OpenSubKey($Path)
    if (!$key) { return $null }
    try { return $key.GetValue($Name) }
    finally { $key.Dispose() }
}
function Write-TestValue([string]$Path, [string]$Name, [string]$Value) {
    $key = $testRoot.CreateSubKey($Path)
    try { $key.SetValue($Name, $Value) }
    finally { $key.Dispose() }
}
$testId = [Guid]::NewGuid().ToString('N')
$registryPath = "Software\VniDrop\FileAssociationTests\$testId"
$directory = Join-Path ([IO.Path]::GetTempPath()) "vnidrop-association-$testId"
[IO.Directory]::CreateDirectory($directory) | Out-Null
$executable = Join-Path $directory 'VniDrop test.exe'
$replacement = Join-Path $directory 'VniDrop updated.exe'
[IO.File]::WriteAllBytes($executable, [byte[]]@())
[IO.File]::WriteAllBytes($replacement, [byte[]]@())
$userRoot = [Microsoft.Win32.Registry]::CurrentUser
$testRoot = $null
$classPath = 'Software\Classes\VniDrop.Native.Invitation'
$openWithPath = 'Software\Classes\.vnd\OpenWithProgids'
$userChoicePath = 'Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.vnd\UserChoice'
try {
    $testRoot = $userRoot.CreateSubKey($registryPath)
    Write-TestValue 'Software\Classes\.vnd' '' 'Other.Invitation'
    Write-TestValue $openWithPath 'Other.Invitation' ''
    Write-TestValue $userChoicePath 'ProgId' 'Other.Invitation'
    Write-TestValue $userChoicePath 'Hash' 'preserve-user-choice'
    Write-TestValue 'Software\RegisteredApplications' 'Other' 'Software\Other\Capabilities'
    $unicode = [char]0x00e9 + [string][char]0x65e5
    $invitation = Join-Path $directory "invitation $unicode & 100%.VND"
    foreach ($profile in @('', (Join-Path $directory "profile $unicode & 'name'.vnd"), ([IO.Path]::GetPathRoot($directory)))) {
        Register-VniDropFileAssociation -UserRoot $testRoot -Executable $executable -ProfileDirectory $profile
        $command = Read-Value "$classPath\shell\open\command"
        $parsed = [VniDrop.AssociationTestArguments]::Parse($command.Replace('%1', $invitation))
        $expected = @($executable)
        if ($profile) { $expected += @('--profile', $profile) }
        $expected += $invitation
        Assert-Equal $parsed $expected 'Shell command lost a spaced/Unicode path or a trailing backslash.'
        Assert-Equal (Read-Value $classPath 'VniDropExecutable') $executable 'Executable ownership was not recorded.'
        Assert-Equal (Read-Value "$classPath\DefaultIcon") ('"' + $executable + '",0') 'File icon does not point to the native executable.'
        Assert-Equal (Read-Value $openWithPath 'VniDrop.Native.Invitation') '' 'Native app is missing from Open with.'
        Assert-Equal (Read-Value 'Software\VniDrop\Native\Capabilities\FileAssociations' '.vnd') 'VniDrop.Native.Invitation' 'Native app is missing from Default apps.'
        Assert-Equal (Read-Value 'Software\Classes\.vnd') 'Other.Invitation' 'Registration changed the existing association.'
        Assert-Equal (Read-Value $userChoicePath 'ProgId') 'Other.Invitation' 'Registration replaced UserChoice.'
        Assert-Equal (Read-Value $userChoicePath 'Hash') 'preserve-user-choice' 'Registration touched the UserChoice hash.'
    }
    $before = Read-Value "$classPath\shell\open\command"
    $rejected = $false
    try { Register-VniDropFileAssociation -UserRoot $testRoot -Executable (Join-Path $directory 'missing.exe') }
    catch { $rejected = $true }
    Assert-Equal $rejected $true 'Registration accepted a missing executable.'
    Assert-Equal (Read-Value "$classPath\shell\open\command") $before 'Failed registration damaged the current command.'
    Register-VniDropFileAssociation -UserRoot $testRoot -Executable $replacement
    Assert-Equal (Unregister-VniDropFileAssociation -UserRoot $testRoot -Executable $executable) $false 'Old installation removed a newer registration.'
    Assert-Equal (Read-Value $classPath 'VniDropExecutable') $replacement 'New installation ownership was lost.'
    [IO.File]::Delete($replacement)
    Assert-Equal (Unregister-VniDropFileAssociation -UserRoot $testRoot -Executable $replacement) $true 'Uninstall failed after executable removal.'
    Assert-Equal (Read-Value "$classPath\shell\open\command") $null 'Uninstall left a stale open command.'
    Assert-Equal (Read-Value $openWithPath 'VniDrop.Native.Invitation') $null 'Uninstall left a stale Open with entry.'
    Assert-Equal (Read-Value 'Software\RegisteredApplications' 'VniDrop.Native') $null 'Uninstall left a stale Default apps entry.'
    Assert-Equal (Read-Value 'Software\VniDrop\Native\Capabilities\FileAssociations' '.vnd') $null 'Uninstall left stale capabilities.'
    Assert-Equal (Read-Value 'Software\RegisteredApplications' 'Other') 'Software\Other\Capabilities' 'Uninstall removed another app.'
    Assert-Equal (Read-Value $openWithPath 'Other.Invitation') '' 'Uninstall removed another file handler.'
    Assert-Equal (Read-Value 'Software\Classes\.vnd') 'Other.Invitation' 'Uninstall changed the existing association.'
    Assert-Equal (Read-Value $userChoicePath 'Hash') 'preserve-user-choice' 'Uninstall changed UserChoice.'
    Assert-Equal (Unregister-VniDropFileAssociation -UserRoot $testRoot -Executable $replacement) $false 'Repeated uninstall should be a no-op.'
    $freshUser = $testRoot.CreateSubKey('FreshUser')
    $machineClasses = $testRoot.CreateSubKey('MachineClasses')
    try {
        $machineExtension = $machineClasses.CreateSubKey('.vnd')
        try { $machineExtension.SetValue('', 'Machine.Invitation') }
        finally { $machineExtension.Dispose() }
        Register-VniDropFileAssociation -UserRoot $freshUser -Executable $executable -MachineClasses $machineClasses
        $extension = $freshUser.OpenSubKey('Software\Classes\.vnd')
        try { Assert-Equal ($extension.GetValue('')) $null 'Registration replaced a machine-wide default.' }
        finally { $extension.Dispose() }
        $machineClasses.DeleteSubKeyTree('.vnd')
        $choice = $freshUser.CreateSubKey($userChoicePath)
        try { $choice.SetValue('ProgId', 'Chosen.Invitation') }
        finally { $choice.Dispose() }
        Register-VniDropFileAssociation -UserRoot $freshUser -Executable $executable -MachineClasses $machineClasses
        $extension = $freshUser.OpenSubKey('Software\Classes\.vnd')
        try { Assert-Equal ($extension.GetValue('')) $null 'Initial mapping ignored an explicit UserChoice.' }
        finally { $extension.Dispose() }
        $freshUser.DeleteSubKeyTree($userChoicePath)
        Register-VniDropFileAssociation -UserRoot $freshUser -Executable $executable -MachineClasses $machineClasses
        $extension = $freshUser.OpenSubKey('Software\Classes\.vnd')
        try { Assert-Equal ($extension.GetValue('')) 'VniDrop.Native.Invitation' 'A new file type has no initial handler.' }
        finally { $extension.Dispose() }
    } finally { $freshUser.Dispose(); $machineClasses.Dispose() }
    Write-Output 'PASS: file registration, Windows argument round trips, existing defaults, missing executables, upgrades, and owned uninstall.'
} finally {
    if ($testRoot) { $testRoot.Dispose(); $userRoot.DeleteSubKeyTree($registryPath, $false) }
    $userRoot.Dispose()
    [IO.File]::Delete($executable)
    [IO.File]::Delete($replacement)
    [IO.Directory]::Delete($directory)
}
