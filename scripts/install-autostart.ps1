# ログオン時に Device HUB を自動起動する (G HUB の常駐の置き換え)
#
#   .\scripts\install-autostart.ps1                 # 登録 (既定: target\release\device-hub.exe)
#   .\scripts\install-autostart.ps1 -Uninstall      # 解除
#   .\scripts\install-autostart.ps1 -Exe "D:\apps\device-hub.exe"
#
# スタートアップ フォルダにショートカットを作るだけなので管理者権限は不要。
# タスクトレイ常駐は未実装 (ウィンドウは通常どおり開く)。

param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\device-hub.exe'),
    [switch]$Uninstall
)

$startup = [Environment]::GetFolderPath('Startup')
$link = Join-Path $startup 'Device HUB.lnk'

if ($Uninstall) {
    if (Test-Path $link) {
        Remove-Item $link
        Write-Host "削除しました: $link"
    } else {
        Write-Host "登録されていません: $link"
    }
    exit 0
}

$Exe = [System.IO.Path]::GetFullPath($Exe)
if (-not (Test-Path $Exe)) {
    Write-Error "exe が見つかりません: $Exe`n先に 'cargo build --release -p device-hub' を実行してください。"
    exit 1
}

$shell = New-Object -ComObject WScript.Shell
$sc = $shell.CreateShortcut($link)
$sc.TargetPath = $Exe
$sc.WorkingDirectory = Split-Path $Exe
$sc.IconLocation = $Exe
$sc.Description = 'Device HUB (HID configurator)'
$sc.Save()

Write-Host "登録しました: $link"
Write-Host "  -> $Exe"
Write-Host "次回のログオンから自動で起動します。今すぐ起動する場合は exe を実行してください。"
