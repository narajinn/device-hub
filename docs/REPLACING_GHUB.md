# Logicool G HUB を Device HUB で置き換える (PRO X SUPERLIGHT)

対象: **G PRO X SUPERLIGHT** (Lightspeed レシーバー `046D:C547`、HID++ 2.0)。

## 1. できること / できないこと

| G HUB の機能 | Device HUB | 備考 |
| --- | --- | --- |
| 機種名・接続状態の表示 | ○ | レシーバーを `PRO X SUPERLIGHT` として表示 |
| バッテリー残量 | ○ | `0x1004` f1。残量 % と充電中フラグ |
| DPI の変更 | ○ | `0x2201` f2/f3。複数ステージも書き込み可 |
| DPI ステージ切替ボタン | ○ (UI から) | マウス側ボタンでの切替はマウス内のプロファイル依存 |
| ポーリングレート表示 | △ | 実測ピーク表示のみ (下記) |
| ポーリングレート変更 | × | ファームが `0x8060` f2 を error `0x02` で拒否 |
| ボタン再割当 / マクロ | × | `0x1B04` 非搭載。`0x8100` 実装が必要 |
| ライティング | × | この機種は LED 無し |
| ファームウェア更新 | × | G HUB 相当の更新サーバは使っていない |

### ポーリングレートについて

`0x8060` の読み値は index 1 (= 250 Hz) を返すが、Raw Input で測った実測ピークは
**1009 Hz**。この個体はオンボードプロファイル側のレートで動作しているため、
HID++ の値は実態を表していない。誤表示を避けるため **設定 UI には出さず**、
アプリ本来の「実測ポーリングレート」表示を使う。

## 2. 常時起動

```powershell
cargo build --release -p device-hub
.\scripts\install-autostart.ps1            # スタートアップにショートカット作成
.\scripts\install-autostart.ps1 -Uninstall # 解除
```

- ショートカットを置くだけなので管理者権限は不要。
- **タスクトレイ常駐は未実装**。ウィンドウを閉じるとプロセスは終了する。
  常駐させたい場合は最小化したまま使う (設定は `%APPDATA%\DeviceHub\prefs.json`)。
- 起動時に最後に使ったデバイス (`last_device`) を開く。

## 3. G HUB を消す前に確認すること

1. `cargo run -p devhub-hid --example dpiwrite` — DPI の往復 (800 → 元の値) が `OK`
2. アプリを起動して DPI スライダー/ステージを変え、**実際にマウスの感度が変わる**
3. 電池残量が G HUB と同じ桁で出る
4. 再ログオンして自動起動する

## 4. G HUB のアンインストール

```powershell
# 1) G HUB 本体とサービスを終了
Get-Process LGHUB, lghub_agent, lghub_updater -ErrorAction SilentlyContinue | Stop-Process -Force

# 2) 公式アンインストーラ
& "C:\Program Files\LGHUB\lghub_software_manager.exe" --uninstall
```

- 残るファイル: `C:\Program Files\LGHUB`, `%LOCALAPPDATA%\LGHUB`, `C:\ProgramData\LGHUB`
- デバイス側の設定 (オンボードプロファイル) はマウスのフラッシュに残るので、
  G HUB を消してもレート・ボタン割当は最後の設定のまま動く。
- 元に戻したくなったら公式サイトから再インストール (設定はデバイス側に残っている)。

## 5. 実装メモ

- ドライバ: `crates/devhub-hid/src/logi.rs`
- プロトコル解析: `docs/LOGITECH_HIDPP.md`
- 実機ダンプ: `cargo run -p devhub-hid --example logiprobe`
- DPI 往復試験: `cargo run -p devhub-hid --example dpiwrite [-- 800]`
- registry: `crates/devhub-core/src/registry.rs` の `046D:C547` (usage `FF00/0002` で絞り込み)

HID++ は `usage_page 0xFF00 / usage 0x0002` のコレクションにだけ通る。
`0xFF00/0x0001` を開くと書き込みは通るが応答が返らないので、列挙時に弾いている。
