# Introduction

This extension is supposed to allow remote debugging using cppvsdbg.

<details>
  <summary><strong>Click to expand</strong></summary>
  
  ```PowerShell
  #Requires -RunAsAdministrator

param(
  [Parameter(Mandatory=$true, HelpMessage="The SSH public key to append")]
  [string]$PublicKey
)

Write-Host "Starting SSH Setup for Administrators..." -ForegroundColor Cyan

$sshDir = "$env:ProgramData\ssh"
$authFile = "$sshDir\administrators_authorized_keys"

# 1. Install OpenSSH Server capability (if not already installed)

$capability = Get-WindowsCapability -Online -Name "OpenSSH.Server~~~~0.0.1.0"
  if ($capability.State -ne 'Installed') {
Write-Host "Installing OpenSSH Server..."
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0 | Out-Null
} else {
Write-Host "OpenSSH Server already installed. Skipping."
}

# 2. Start service and set to Automatic

Write-Host "Configuring sshd service..."
Set-Service -Name sshd -StartupType 'Automatic'
Start-Service sshd -ErrorAction SilentlyContinue

# 3. Create the administrators_authorized_keys file

if (-not (Test-Path $sshDir)) {
New-Item -Path $sshDir -ItemType Directory | Out-Null
}
if (-not (Test-Path $authFile)) {
New-Item -Path $authFile -ItemType File | Out-Null
Write-Host "Created $authFile"
}

# 4. Set the strictly required ACLs

Write-Host "Applying strict ACLs..."
$acl = Get-Acl $authFile

# Disable inheritance and remove inherited rules

$acl.SetAccessRuleProtection($true, $false)

# Grant FullControl to SYSTEM

$ruleSystem = New-Object System.Security.AccessControl.FileSystemAccessRule("SYSTEM", "FullControl", "Allow")
$acl.AddAccessRule($ruleSystem)

# Grant FullControl to the local Administrators group


$sid = New-Object System.Security.Principal.SecurityIdentifier([System.Security.Principal.WellKnownSidType]::BuiltinAdministratorsSid, $null)
$identity = $sid.Translate([System.Security.Principal.NTAccount])
$ruleAdmins = New-Object System.Security.AccessControl.FileSystemAccessRule($identity, "FullControl", "Allow")
$acl.AddAccessRule($ruleAdmins)

Set-Acl -Path $authFile -AclObject $acl

# 5. Write the provided public key (Idempotent check)

$currentKeys = ""
if (Test-Path $authFile) {
$currentKeys = Get-Content $authFile -Raw
}

if ($currentKeys -match [regex]::Escape($PublicKey)) {
Write-Host "Public key is already present in the file. Skipping addition." -ForegroundColor Yellow
} else {
Write-Host "Appending new public key..."
Add-Content -Path $authFile -Value $PublicKey
}

# Restart to ensure everything is picked up

Write-Host "Restarting sshd service to apply changes..."
Restart-Service sshd

Write-Host "Done! The target machine is ready for remote SSH connections." -ForegroundColor Green

```
</details>
```
