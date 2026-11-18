# Logitech HID++ 2.0 — PRO X SUPERLIGHT (実機解析メモ)

対象: Logicool G PRO X SUPERLIGHT (Lightspeed レシーバー `046D:C547`)
解析日: 2026-11-18 / 実機で取得した生フレームのみ記載 (推測は「推定」と明記)

## 1. 接続先インターフェース

レシーバーは HID コレクションを 2 つ持つ。**HID++ が通るのは Col02 だけ**だった。

| コレクション | usage_page / usage | 結果 |
| --- | --- | --- |
| `MI_02&Col01` | `FF00` / `0001` | 長短とも無応答 (short `0x10` のみ HID++1.0 のエラー `8F` が返る) |
| `MI_02&Col02` | `FF00` / `0002` | **長レポート `0x11` で HID++ 2.0 応答あり** |

- `Col02` でも **short レポート `0x10` は無応答**。全リクエストを長レポート (20 byte) で送る。
- hidapi の `read()` はレポート ID (`0x11`) を含んで返る。Windows では `write()` の先頭バイトがレポート ID。
- デバイスインデックスは `0x01` (マウス)。`0xFF` (レシーバー自身) は `Col02` では無応答だった。
- `0x02` / `0x03` は未接続スロットで、`8F 00 00 08` (unknown device) が返る。

## 2. フレーム

```
request : [0x11, dev, featureIndex, (function<<4)|swId, params...]  → 20 byte にゼロ詰め
response: [0x11, dev, featureIndex, (function<<4)|swId, params...]
error   : [0x11, dev, 0xFF, featureIndex, (function<<4)|swId, errorCode, ...]
```

- `swId` は任意 (解析では `0x0A` 固定)。応答の下位ニブルにそのまま返る。
- 機能インデックスは起動ごとに変わりうるので、必ず `IRoot.getFeature(featureId)` で引く。

## 3. 機能一覧 (実機の応答そのまま)

`IRoot(0x0000).getFeature(0x0001)` → idx `0x01` → `getCount()` = **27**。

| idx | featureId | 名前 | 状態 |
| --- | --- | --- | --- |
| 01 | 0x0001 | IFeatureSet | 取得OK |
| 02 | 0x0003 | IDeviceName | 取得OK |
| 03 | 0x0005 | IDeviceTypeAndName | 応答は返るが名前長 0 |
| 04 | 0x1D4B | (Logitech 固有 / PRO 系) | 未解析 |
| 05 | 0x0020 | (Logitech 固有) | 未解析 |
| 06 | 0x1004 | UnifiedBattery | 取得OK |
| 07 | 0x8070 | (レポートレート系の別機能) | 未解析 |
| 08 | 0x8100 | OnboardProfiles | 取得OK |
| 09 | 0x8110 | (プロファイル関連) | 未解析 |
| 10 | 0x8060 | ReportRate | 取得OK |
| 11 | 0x2201 | AdjustableDpi | 取得OK |
| 12 | 0x1500 | (Logitech 固有) | 未解析 |
| 13 | 0x1802 | LEDControl | あるが GPX に RGB は無い |
| 14 | 0x1803 | (LED) | 未使用 |
| 15 | 0x1806 | ConfigChange | 変更通知用 |
| 16 | 0x1811 | (未解析) | — |
| 17 | 0x1805 | Reset | 工場リセット |
| 18 | 0x1830 | (未解析) | — |
| 19 | 0x1890 | (未解析) | — |
| 20 | 0x1891 | (未解析) | — |
| 21 | 0x18A1 | (未解析) | — |
| 22 | 0x1801 | (未解析) | — |
| 23 | 0x18B1 | (未解析) | — |
| 24 | 0x1E00 | EnableHiddenFeatures | 有効化すれば追加機能が見える可能性 |
| 25 | 0x1EB0 | (未解析) | — |
| 26 | 0x1863 | (未解析) | — |

**重要**: `0x1B04` (ReprogrammableKeys / ボタン割当) は **存在しない** (getFeature が featureIndex 0 を返す)。
→ ボタン再割当は HID++ のキー機能ではなく **0x8100 オンボードプロファイル側**にしかない。
GPX でボタン変更を出すなら 0x8100 のセクター読み書き実装が必要 (未着手)。

## 4. 実測値

### デバイス名 (0x0003 / 0x0005)

この個体は **`0x0003 IDeviceName` が壊れている**。`getFeature(0x0003)` は index 2 を
返すが、index 2 の f0/f1 は名前ではなく意味不明なバイト列を返す。名前は
**`0x0005 IDeviceTypeAndName`** から取れる:

```
>> 11 01 03 0a            f0 (index 3 = 0x0005) → 長さ
<< 11 01 03 0a 0e         = 14
>> 11 01 03 1a 00         f1 offset 0
<< 11 01 03 1a 50 52 4f 20 58 20 57 69 72 65 6c 65 73 73 00 …
   = "PRO X Wireless"
```

ドライバは `0x0005` → `0x0003` の順に試し、印字可能 ASCII でなければ捨てる
(`plausible_name`)。

### 機能インデックスは接続ごとに変わる

同じ個体でもセッションによって機能表が変わった (例: `0x1E22` が増えて 27 機能に
なった回がある)。**インデックスをキャッシュしない**こと。`Logi::open` は毎回
`IRoot.getFeature` で 4 機能 (0x0003/0x0005・0x1004・0x2201・0x8060) を引き直す。

### ファーム/プロトコル (IRoot)
```
>> 11 01 00 1a
<< 11 01 00 1a 04 02         HID++ 2.0 / version 4.2
```

### バッテリー (0x1004)
```
getCapabilities f0: 11 01 06 0a 0f 03      → levels=15, flags=0x03 (rechargeable)
getStatus       f1: 11 01 06 1a 21 04      → level=0x21 (33%), status=0x04
```
`level` はパーセント。`status` の 4 は「放電中」相当 (要: 充電中の値と比較)。

### DPI (0x2201)
```
f0 sensorCount   : 11 01 0b 0a 01                    → 1 センサー
f2 sensorDpi     : 11 01 0b 2a 00 06 40 03 20        → sensor0: 現在 0x0640=1600, 既定 0x0320=800
f3 setSensorDpi  : params [sensorIdx, dpi_hi, dpi_lo] で書ける (params 無しだと ERR 0x02)
f1 sensorDpiList : 11 01 0b 1a 00 00 64 e0 32 64     → sensor0, 100 と 0xE032?? (エンコード要確認)
```
DPI は 16bit ビッグエンディアン。`setSensorDpi` は書き込み後 `getSensorDpi` で往復確認する。

### レポートレート (0x8060)
```
f0 getCapabilities: 11 01 0a 0a 8b      → 対応レートのビットマスク 0x8b (bit0,1,3,7)
f1 getReportRate  : 11 01 0a 1a 01      → 現在 index 1
```
ビットマスクは固定テーブル `[125, 250, 500, 1000, 2000, 4000, 8000, 16000]` の index と推定。
GPX (初代) は 1000Hz 上限なので `0x8b` = {125, 250, 1000} と整合。書き込みは §6 のとおり拒否される。

### オンボードプロファイル (0x8100)
```
f0 profileFormat: 11 01 08 0a 01 04 01 05 01 05 10 00 ff 0a 04
   → sectorSize=0x0104? / profileCount=1? (Logitech の profile format 仕様に照合中)
f1 getProfile    : 全ゼロ (プロファイル 0 の中身)
f2 getProfileInfo: 11 01 08 2a 01 00      → profile 0 が有効
f4 mode          : 11 01 08 4a 00 01      → onboard mode = 1 (onboard)
```
ボタン割当・マクロを出すにはここを実装する必要がある (今回は未実装)。

## 5. G HUB との共存

- G HUB (LGHUBUpdaterService) が動いていても上記の**読み取りは成功**した。
- 書き込み (DPI / レート) は G HUB と取り合う可能性があるため、検証時は G HUB 本体を終了する。
- G HUB を消すのは**本アプリで DPI・レート・電池が往復確認できた後**。

## 6. 書き込みの実機検証 (2026-11-18)

### DPI — 成功

```
>> 11 01 0b 3a 00 03 20        setSensorDpi(sensor=0, 800)
<< 11 01 0b 3a 00 03 20        (echo)
   → getSensorDpi = 00 03 20 = 800 で往復確認
>> 11 01 0b 3a 00 06 40        setSensorDpi(1600) で復帰
```

DPI は `[sensorIdx, dpiHi, dpiLo]` の 3 byte。書き込み後に `getSensorDpi` で確認できる。
**DPI はアプリから変更可能**。

### レポートレート — 変更不可

```
>> 11 01 0a 2a 03              setReportRate(3)
<< 11 01 ff 0a 3a 02           HID++ error 0x02 (INVALID_ARGUMENT)
```

`0x8060` の function 2 は **どの値 (0〜8) でも error 0x02** で拒否された。
`setMode` でオンボードモードを host (0) にしても変わらず、function 3 以降は
error 0x07 (INVALID_FUNCTION)。

さらに **読み値も実態と一致しない**:

```
getRate  : 11 01 0a 1a → index 1 (= 250 Hz)
Raw Input の実測ピーク: 1009 Hz
```

つまりこの個体はオンボードプロファイル側のレートで動作していて、`0x8060` は
「ホスト設定」の値しか返していない。誤った数値を UI に出さないよう、
**レート設定は UI から外した** (アプリ本来の「実測ピーク」表示の方が正確)。
`Logi::polling_rate()` は診断用に残してある。

### 電池

`getStatus` は `level=0x21 (33%)`, `status=0x04`。status 0x04 は放電中と推定
(充電中の値は未採取)。ドライバでは `1..=3` を充電中として扱っている。

## 7. アプリに入れた範囲 / 残り

入れた:

- 機種名 (`0x0005` → `0x0003` フォールバック)、ファーム (= HID++ バージョン)
- 電池 (`0x1004` f1: level / charging)
- DPI 読み書き (`0x2201` f2 / f3) — DPI ステージはアプリ側の複数ステージをそのまま書き込む
- `DriverKind::Logi` + registry (`046D:C547`, usage `FF00/0002`)

残り (やるなら):

1. ボタン割当 — `0x1B04` 非搭載。`0x8100` オンボードプロファイルの
   セクター (sectorSize/`0x0104`) を読んで構造を特定する必要がある
2. レート変更 — ファームが拒否するので不可 (オンボード側の値を書き換えるしかない)
3. 充電中の電池ステータス値の採取 (`getStatus` f1 の 2 byte 目)
4. LOD / デバウンス等 — この機種には該当機能が無い (`0x1E22` 等を確認)
