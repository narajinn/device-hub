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
- 起動するとタスクトレイに常駐する。**最小化するとタスクトレイに格納**され、
  タスクバーのボタンは消える (タスクトレイのアイコンを左クリック、または
  右クリック → 「Device HUB を開く」で戻る)。
- Windows 11 は新しいトレイ アイコンを既定で「^」(非表示のアイコン) に入れる。
  常時見えるようにするには、タスクバーの「^」を開いてアイコンをタスクバーへ
  ドラッグするか、設定 → 個人用設定 → タスクバー → 「その他のシステム トレイ アイコン」
  で Device HUB をオンにする。
- × で閉じると終了する (常駐を続けたいときは最小化を使う)。
- 設定は `%APPDATA%\DeviceHub\prefs.json`。起動時に最後に使ったデバイスを開く。

## 3. G HUB を消す前に確認すること

1. `cargo run -p devhub-hid --example dpiwrite` — DPI の往復 (800 → 元の値) が `OK`
2. アプリを起動して DPI スライダー/ステージを変え、**実際にマウスの感度が変わる**
3. 電池残量が G HUB と同じ桁で出る
4. 再ログオンして自動起動する

## 4. G HUB のアンインストール

実際に実行した手順 (2026-11-19):

```powershell
# 1) 常駐プロセスを止める (lghub_updater は管理者権限なので残る → 手順 2 が止める)
Get-Process lghub, lghub_agent, lghub_system_tray -ErrorAction SilentlyContinue | Stop-Process -Force

# 2) 公式アンインストーラーを管理者として起動し、画面の指示に従う
Start-Process "C:\Program Files\LGHUB\lghub_software_manager.exe" -ArgumentList "--uninstall" -Verb RunAs
```

- `winget uninstall Logitech.GHUB` は **MSI エラー 1287 で失敗**する (G HUB は MSI ではない)。
  公式アンインストーラーを直接起動すること。
- 削除後に消えるもの: `C:\Program Files\LGHUB`、`C:\ProgramData\LGHUB`、アンインストール
  レジストリ、`LGHUBUpdaterService` などのサービス。
- 残るもの: `%LOCALAPPDATA%\LGHUB` (設定とキャッシュ)。不要なら手で消してよい。
- デバイス側の設定 (オンボードプロファイル) はマウスのフラッシュに残るので、
  G HUB を消してもレート・ボタン割当は最後の設定のまま動く。

### 削除後に再確認したこと

```
$ cargo run -p devhub-hid --example logiprobe
  name     : PRO X SUPERLIGHT
  firmware : 4.2
  dpi      : 1600
  battery  : 30% charging=true

$ cargo run -p devhub-hid --example dpiwrite
  before : 1600 dpi
  apply  : 800 -> read 800
  restore: 1600 -> read 1600
  OK
```

G HUB が無い状態でも読み取り・DPI 書き込みは通る。

## 5. 実装メモ

- ドライバ: `crates/devhub-hid/src/logi.rs`
- プロトコル解析: `docs/LOGITECH_HIDPP.md`
- 実機ダンプ: `cargo run -p devhub-hid --example logiprobe`
- DPI 往復試験: `cargo run -p devhub-hid --example dpiwrite [-- 800]`
- registry: `crates/devhub-core/src/registry.rs` の `046D:C547` (usage `FF00/0002` で絞り込み)

HID++ は `usage_page 0xFF00 / usage 0x0002` のコレクションにだけ通る。
`0xFF00/0x0001` を開くと書き込みは通るが応答が返らないので、列挙時に弾いている。
