$ErrorActionPreference = 'Stop'
$logDirectory = New-Item -ItemType Directory -Force -Path 'lifecycle-logs'
$executable = Join-Path $env:ProgramFiles 'AirSlate PC Server/airslate_pc_server.exe'
$installer = Get-Item 'target/debug/bundle/msi/*.msi'

Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class AirSlateWindow {
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern IntPtr FindWindow(string className, string title);
    [DllImport("user32.dll")]
    public static extern IntPtr SendMessageTimeout(IntPtr window, uint message,
        UIntPtr wParam, IntPtr lParam, uint flags, uint timeout, out UIntPtr result);
    private delegate bool WindowCallback(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")]
    private static extern bool EnumWindows(WindowCallback callback, IntPtr parameter);
    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr window, StringBuilder text, int capacity);
    public static string DescribeWindows(int processId) {
        var output = new StringBuilder();
        EnumWindows((window, parameter) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == processId) {
                var title = new StringBuilder(1024);
                GetWindowText(window, title, title.Capacity);
                output.AppendLine(window + " " + title);
            }
            return true;
        }, IntPtr.Zero);
        return output.ToString();
    }
}
'@

function Wait-Until([scriptblock] $Condition, [string] $Description) {
    $deadline = [DateTime]::UtcNow.AddSeconds(60)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    Get-Process airslate_pc_server -ErrorAction SilentlyContinue | Format-List Id, Path, Responding
    Get-Process airslate_pc_server -ErrorAction SilentlyContinue | ForEach-Object {
        [AirSlateWindow]::DescribeWindows($_.Id)
    }
    Get-NetTCPConnection -LocalPort 48562 -ErrorAction SilentlyContinue | Format-List
    Get-ChildItem -Path $logDirectory.FullName -Filter '*.stdout.log' | ForEach-Object {
        Write-Output $_.Name
        Get-Content -LiteralPath $_.FullName -Tail 20
    }
    throw "Timed out waiting for $Description"
}

function Invoke-Msi([string[]] $Arguments, [string] $LogName) {
    $logPath = Join-Path $logDirectory.FullName $LogName
    $installation = Start-Process msiexec.exe -WindowStyle Hidden -PassThru -ArgumentList (
        $Arguments + @('/qn', '/norestart', '/l*v', "`"$logPath`"")
    )
    if (-not $installation.WaitForExit(120000)) {
        throw 'Windows Installer did not finish within two minutes'
    }
    if ($installation.ExitCode -ne 0) {
        throw "Windows Installer returned $($installation.ExitCode). See $logPath"
    }
}

function Start-AirSlate([string] $Name, [switch] $Autostart) {
    $parameters = @{
        FilePath = $executable
        WindowStyle = 'Hidden'
        PassThru = $true
        RedirectStandardOutput = Join-Path $logDirectory.FullName "$Name.stdout.log"
        RedirectStandardError = Join-Path $logDirectory.FullName "$Name.stderr.log"
    }
    if ($Autostart) { $parameters.ArgumentList = '--autostart' }
    Start-Process @parameters
}

function Get-ConsoleWindow {
    [AirSlateWindow]::FindWindow([NullString]::Value, 'AirSlate 控制台')
}

function Test-ServerReady {
    $servers = @(Get-Process airslate_pc_server -ErrorAction SilentlyContinue)
    if ($servers.Count -ne 1) { return $false }
    $listeners = @(Get-NetTCPConnection -LocalPort 48562 -State Listen -ErrorAction SilentlyContinue)
    $listeners.Count -eq 1 -and $listeners[0].OwningProcess -eq $servers[0].Id
}

# Tauri restores the unpatched build output after bundling. Use the actual MSI payload
# as the expected installed binary, including its package-type marker.
$payloadDirectory = Join-Path $env:RUNNER_TEMP 'airslate-msi-payload'
Invoke-Msi @('/a', "`"$($installer.FullName)`"", "TARGETDIR=`"$payloadDirectory`"") 'extract.log'
$expectedExecutable = @(Get-ChildItem $payloadDirectory -Recurse -Filter 'airslate_pc_server.exe')
if ($expectedExecutable.Count -ne 1) { throw 'The MSI must contain exactly one AirSlate executable' }

# Exercise the released version's tray-only process during a real same-version MSI upgrade.
gh release download v1.8.0 --repo 2doright/airslate-pc-server `
    --pattern 'AirSlate-PC-Server_1.8.0_windows-x64.msi' --dir $logDirectory.FullName
if ($LASTEXITCODE -ne 0) { throw 'Could not download the v1.8.0 upgrade fixture' }
$oldInstaller = Join-Path $logDirectory.FullName 'AirSlate-PC-Server_1.8.0_windows-x64.msi'
Invoke-Msi @('/i', "`"$oldInstaller`"") 'install-old.log'
$oldServer = Start-AirSlate 'old-autostart' -Autostart
Wait-Until { Test-ServerReady } 'the released server to listen'
Invoke-Msi @('/i', "`"$($installer.FullName)`"") 'upgrade.log'
if ((Get-FileHash $executable).Hash -ne (Get-FileHash $expectedExecutable[0].FullName).Hash) {
    throw 'Manual upgrade did not install the newly built executable'
}
$oldServer.Refresh()
if (-not $oldServer.HasExited) { throw 'Manual upgrade left the old background process running' }

# Launch concurrently from a stopped state, including an autostart invocation.
$launches = @(Start-AirSlate 'cold-autostart' -Autostart)
1..5 | ForEach-Object { $launches += Start-AirSlate "cold-$_" }
Wait-Until { (Test-ServerReady) -and (Get-ConsoleWindow) -ne [IntPtr]::Zero } 'one server and its console'
Wait-Until {
    @($launches | Where-Object { $_.Refresh(); -not $_.HasExited }).Count -eq 1
} 'secondary launches to exit'
$primary = Get-Process airslate_pc_server

# Closing the console releases the webview while the same backend keeps listening.
$messageResult = [UIntPtr]::Zero
$sent = [AirSlateWindow]::SendMessageTimeout((Get-ConsoleWindow), 0x0010,
    [UIntPtr]::Zero, [IntPtr]::Zero, 2, 10000, [ref] $messageResult)
if ($sent -eq [IntPtr]::Zero) { throw 'The console did not respond to WM_CLOSE' }
Wait-Until { (Get-ConsoleWindow) -eq [IntPtr]::Zero } 'the console to close'
if (-not (Test-ServerReady)) { throw 'Closing the console stopped the server' }

$reopens = @(1..5 | ForEach-Object { Start-AirSlate "reopen-$_" })
Wait-Until { (Get-ConsoleWindow) -ne [IntPtr]::Zero } 'the console to reopen'
Wait-Until {
    @($reopens | Where-Object { $_.Refresh(); -not $_.HasExited }).Count -eq 0
} 'all reopening processes to exit'
if (-not (Test-ServerReady) -or (Get-Process airslate_pc_server).Id -ne $primary.Id) {
    throw 'Reopening the console changed the backend process or its listener'
}

# A direct installer launch must also close the fixed version with the console open.
Invoke-Msi @('/i', "`"$($installer.FullName)`"") 'reinstall.log'
Wait-Until { @(Get-Process airslate_pc_server -ErrorAction SilentlyContinue).Count -eq 0 } 'the installed server to exit'

# Force autostart to own the instance lock, then activate it with one manual launch.
$autostartServer = Start-AirSlate 'autostart-primary' -Autostart
Wait-Until { Test-ServerReady } 'the autostart server to listen'
if ((Get-ConsoleWindow) -ne [IntPtr]::Zero) { throw 'Autostart opened the console' }
$manualOpen = Start-AirSlate 'activate-autostart'
Wait-Until { (Get-ConsoleWindow) -ne [IntPtr]::Zero } 'one manual launch to activate the autostart server'
Wait-Until { $manualOpen.Refresh(); $manualOpen.HasExited } 'the manual activation process to exit'
if ((Get-Process airslate_pc_server).Id -ne $autostartServer.Id) {
    throw 'Manual activation replaced the autostart backend'
}
Invoke-Msi @('/i', "`"$($installer.FullName)`"") 'close-autostart.log'
Wait-Until { @(Get-Process airslate_pc_server -ErrorAction SilentlyContinue).Count -eq 0 } 'the autostart server to exit'
Write-Output 'Windows lifecycle tests passed'
