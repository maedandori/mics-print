param(
 [Parameter(Mandatory=$true)][string]$Stage,
 [Parameter(Mandatory=$true)][string]$Target,
 [Parameter(Mandatory=$true)][int]$TrayPid,
 [Parameter(Mandatory=$true)][int]$UpdaterPid
)
$ErrorActionPreference = 'Stop'
$files = @('Print-Agent.exe','Print-Settings.exe')
$backup = Join-Path $Stage 'backup'
$success = $false
$trayStopped = $false
$reason = ''
function UpdateLog([string]$message) {
 try { Add-Content -LiteralPath (Join-Path $Target 'rust-update.log') -Encoding UTF8 -Value ("{0} {1}" -f (Get-Date -Format s),$message) } catch {}
}
function UpdateProgress([int]$percent,[string]$stage,[string]$message) {
 try {
  $json=@{percent=$percent;stage=$stage;message=$message}|ConvertTo-Json -Compress
  $encoding=New-Object System.Text.UTF8Encoding($false)
  $tmp=Join-Path $Target 'rust-update-progress.json.tmp'
  $dest=Join-Path $Target 'rust-update-progress.json'
  [IO.File]::WriteAllText($tmp,$json,$encoding)
  if(Test-Path -LiteralPath $dest){Remove-Item -LiteralPath $dest -Force}
  Move-Item -LiteralPath $tmp -Destination $dest
 } catch {}
}
try {
 foreach($file in $files) {
  if(-not (Test-Path -LiteralPath (Join-Path $Stage $file) -PathType Leaf)) {
   throw "Update package missing $file"
  }
 }
 # Never interrupt a running print job: Core must exit gracefully first.
 $pidFile = Join-Path $Target 'rust-core-active.pid'
 if(Test-Path -LiteralPath $pidFile) {
   $corePid = 0
   if([int]::TryParse((Get-Content -LiteralPath $pidFile -Raw).Trim(),[ref]$corePid)) {
     if(Get-Process -Id $corePid -ErrorAction SilentlyContinue) {
       throw 'Core is still running; update deferred'
     }
   }
 }
 UpdateProgress 91 'waiting-settings' 'Đang chờ đóng Print Settings để cài đặt an toàn...'
 # Do not terminate the Settings GUI or replace an EXE it may still be using.
 $closed = $false
 for($try=0;$try -lt 60;$try++) {
   if(-not (Get-Process -Name 'Print-Settings' -ErrorAction SilentlyContinue)) {$closed=$true;break}
   Start-Sleep -Seconds 2
 }
 if(-not $closed) {throw 'Print Settings still open; update deferred'}
 UpdateProgress 94 'installing' 'Đang sao lưu và thay thế hai EXE...'
 # Stop only the tray process. Do not stop unrelated C# apps.
 if($TrayPid -gt 0) {Stop-Process -Id $TrayPid -ErrorAction SilentlyContinue; $trayStopped=$true}
 if($UpdaterPid -gt 0) {
   try {Wait-Process -Id $UpdaterPid -Timeout 30 -ErrorAction SilentlyContinue} catch {}
 }
 Start-Sleep -Milliseconds 600
 New-Item -ItemType Directory -Path $backup -Force | Out-Null
 foreach($file in $files) {
   $old=Join-Path $Target $file
   if(Test-Path -LiteralPath $old) {
     Copy-Item -LiteralPath $old -Destination (Join-Path $backup $file) -Force -ErrorAction Stop
   }
 }
 # Retry briefly for antivirus / shell release of EXE handles.
 $applied=$false
 for($try=0;$try -lt 30;$try++) {
   try {
     foreach($file in $files) {
       Copy-Item -LiteralPath (Join-Path $Stage $file) -Destination (Join-Path $Target $file) -Force -ErrorAction Stop
     }
     $applied=$true
     break
   } catch {Start-Sleep -Seconds 2}
 }
 if(-not $applied){throw 'Unable to replace executables after retry'}
 $success=$true
 UpdateProgress 99 'restarting' 'Đã cài đặt xong; đang khởi động lại Print Agent...'
 UpdateLog 'Automatic Rust update installed successfully, restarting Print Agent'
} catch {
 $reason=$_.Exception.Message
 foreach($file in $files) {
   $old=Join-Path $backup $file
   if(Test-Path -LiteralPath $old) {
     try {Copy-Item -LiteralPath $old -Destination (Join-Path $Target $file) -Force -ErrorAction Stop} catch {}
   }
 }
 UpdateProgress 0 'error' ("Cập nhật bị hoãn hoặc thất bại: $reason")
 UpdateLog ("Automatic update deferred/failed, rollback attempted: $reason")
} finally {
 # Never alter config.ini, agent-id.txt, xlsx, job state or PDF files.
 if($success -or $trayStopped) {
   $agent=Join-Path $Target 'Print-Agent.exe'
   if(Test-Path -LiteralPath $agent) {
     try {Start-Process -FilePath $agent -WorkingDirectory $Target} catch {UpdateLog "Agent restart failed: $_"}
   }
 }
 if($success) {
   UpdateProgress 100 'completed' 'Cập nhật thành công, Print Agent đã được khởi động lại'
   try {Remove-Item -LiteralPath $Stage -Recurse -Force -ErrorAction SilentlyContinue} catch {}
 }
}
