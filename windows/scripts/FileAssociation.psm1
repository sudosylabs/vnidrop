Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$progId = 'VniDrop.Native.Invitation'
$capabilitiesPath = 'Software\VniDrop\Native\Capabilities'

function ConvertTo-AssociationArgument([string]$Value) {
    if ($Value.IndexOfAny([char[]]"`"`r`n`0") -ge 0) { throw 'Invalid character in an association path.' }
    return '"' + ($Value -replace '(\\+)$', '$1$1') + '"'
}

function Set-AssociationValue([Microsoft.Win32.RegistryKey]$Root, [string]$Path, [string]$Name, [string]$Value) {
    $key = $Root.CreateSubKey($Path)
    try { $key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String) }
    finally { $key.Dispose() }
}

function Register-VniDropFileAssociation {
    param(
        [Parameter(Mandatory)][Microsoft.Win32.RegistryKey]$UserRoot,
        [Parameter(Mandatory)][string]$Executable,
        [string]$ProfileDirectory,
        [Microsoft.Win32.RegistryKey]$MachineClasses
    )
    $executablePath = [IO.Path]::GetFullPath($Executable)
    if (!(Test-Path -LiteralPath $executablePath -PathType Leaf) -or [IO.Path]::GetExtension($executablePath) -ine '.exe') {
        throw 'The native VniDrop executable must exist before registering it.'
    }
    $command = ConvertTo-AssociationArgument $executablePath
    if ($ProfileDirectory) { $command += ' --profile ' + (ConvertTo-AssociationArgument ([IO.Path]::GetFullPath($ProfileDirectory))) }
    $command += ' "%1"'
    $icon = (ConvertTo-AssociationArgument $executablePath) + ',0'
    $classPath = "Software\Classes\$progId"
    Set-AssociationValue $UserRoot $classPath '' 'VniDrop (.vnd)'
    Set-AssociationValue $UserRoot $classPath 'VniDropExecutable' $executablePath
    Set-AssociationValue $UserRoot "$classPath\DefaultIcon" '' $icon
    Set-AssociationValue $UserRoot "$classPath\Application" 'ApplicationName' 'VniDrop'
    Set-AssociationValue $UserRoot "$classPath\Application" 'ApplicationIcon' $icon
    Set-AssociationValue $UserRoot "$classPath\shell" '' 'open'
    Set-AssociationValue $UserRoot "$classPath\shell\open\command" '' $command
    Set-AssociationValue $UserRoot 'Software\Classes\.vnd\OpenWithProgids' $progId ''
    Set-AssociationValue $UserRoot $capabilitiesPath 'ApplicationName' 'VniDrop'
    Set-AssociationValue $UserRoot $capabilitiesPath 'ApplicationDescription' 'VniDrop'
    Set-AssociationValue $UserRoot $capabilitiesPath 'ApplicationIcon' $icon
    Set-AssociationValue $UserRoot "$capabilitiesPath\FileAssociations" '.vnd' $progId
    Set-AssociationValue $UserRoot 'Software\RegisteredApplications' 'VniDrop.Native' $capabilitiesPath
    $extension = $UserRoot.OpenSubKey('Software\Classes\.vnd')
    $choice = $UserRoot.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.vnd\UserChoice')
    $machineExtension = if ($MachineClasses) { $MachineClasses.OpenSubKey('.vnd') } else { $null }
    try {
        # Supply the proprietary file type's initial mapping, but never replace a default or UserChoice.
        if (!$extension.GetValue('') -and (!$choice -or !$choice.GetValue('ProgId')) -and
            (!$machineExtension -or !$machineExtension.GetValue(''))) {
            Set-AssociationValue $UserRoot 'Software\Classes\.vnd' '' $progId
        }
    } finally {
        $extension.Dispose()
        if ($choice) { $choice.Dispose() }
        if ($machineExtension) { $machineExtension.Dispose() }
    }
}

function Unregister-VniDropFileAssociation {
    param(
        [Parameter(Mandatory)][Microsoft.Win32.RegistryKey]$UserRoot,
        [Parameter(Mandatory)][string]$Executable
    )
    $classPath = "Software\Classes\$progId"
    $key = $UserRoot.OpenSubKey($classPath)
    if (!$key) { return $false }
    try { $owner = $key.GetValue('VniDropExecutable') }
    finally { $key.Dispose() }
    # An old build's removal must not unregister a newer installation.
    if (![string]::Equals($owner, [IO.Path]::GetFullPath($Executable), [StringComparison]::OrdinalIgnoreCase)) { return $false }
    $UserRoot.DeleteSubKeyTree($classPath, $false)
    $UserRoot.DeleteSubKeyTree($capabilitiesPath, $false)
    foreach ($entry in @(
        @{ Path = 'Software\Classes\.vnd\OpenWithProgids'; Name = $progId },
        @{ Path = 'Software\RegisteredApplications'; Name = 'VniDrop.Native' }
    )) {
        $key = $UserRoot.OpenSubKey($entry.Path, $true)
        if ($key) {
            try { $key.DeleteValue($entry.Name, $false) }
            finally { $key.Dispose() }
        }
    }
    return $true
}

Export-ModuleMember -Function Register-VniDropFileAssociation, Unregister-VniDropFileAssociation
