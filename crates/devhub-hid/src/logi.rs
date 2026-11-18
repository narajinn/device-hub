//! Logitech HID++ 2.0 系 (PRO X SUPERLIGHT / Lightspeed レシーバー) ドライバ。
//!
//! 実機解析の生フレームは `docs/LOGITECH_HIDPP.md`。要点:
//!
//! - 設定チャネルは **usage_page `FF00` / usage `0002` のコレクション**だけ。
//!   同じインターフェースの `0001` コレクションは HID++ 2.0 に応答しない。
//! - **長レポート `0x11` (20 byte) のみ**応答する。short `0x10` は無応答。
//! - レシーバー経由なのでデバイスインデックスは `0x01` (マウス)。`0xFF` は応答しない。
//! - 機能は起動ごとにインデックスが変わるため、`IRoot.getFeature` で引いてから使う。
//!
//! 実装済み: 機種名 / 電池 (`0x1004`) / DPI (`0x2201`) / レポートレート (`0x8060`)。
//! 未実装: ボタン割当 (この機種は `0x1B04` を持たず、`0x8100` オンボードプロファイル
//! 側にしかない)、マクロ、ライティング。

use devhub_core::backend::*;
use devhub_core::model::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 長レポートのレポート ID。
const RID_LONG: u8 = 0x11;
/// ソフトウェア ID (応答の下位ニブルにそのまま返る)。任意の固定値。
const SW_ID: u8 = 0x0A;
/// 応答待ちの全体予算。無線のスリープ復帰を挟むと 1 回 300ms 程度かかることがある。
const REPLY_BUDGET: Duration = Duration::from_millis(700);
/// `read_timeout` 1 回の待ち。
const READ_SLICE_MS: i32 = 40;

/// HID++ 2.0 のレポートレート index → Hz。`0x8060 getCapabilities` のビット位置に対応。
const RATE_TABLE: [u16; 8] = [125, 250, 500, 1000, 2000, 4000, 8000, 16000];

/// 名前として妥当か (`0x0003` が壊れている個体の応答を弾く)。
fn plausible_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_graphic() || c == ' ')
        && s.chars().any(|c| c.is_ascii_alphabetic())
}

/// 1 台分のリンクと機能インデックス。
pub struct Logi {
    dev: Mutex<hidapi::HidDevice>,
    info: Mutex<DeviceInfo>,
    /// レシーバー内のデバイスインデックス (通常 1)。
    dev_index: u8,
    /// `IDeviceName (0x0003)`
    name_id: u8,
    /// `IDeviceTypeAndName (0x0005)`
    type_name_id: u8,
    /// `AdjustableDpi (0x2201)`
    dpi_id: u8,
    /// `ReportRate (0x8060)`
    rate_id: u8,
    /// `UnifiedBattery (0x1004)`
    batt_id: u8,
    /// 解析用に取れた HID++ のバージョン ("4.2" 等)。
    protocol: Option<String>,
    /// デバイス自身が名乗る名前 ("PRO X Wireless")。表示名は registry 側を使う。
    hidpp_name: Option<String>,
}

pub fn open(dev: hidapi::HidDevice, info: DeviceInfo) -> HubResult<Logi> {
    Logi::open(dev, info)
}

impl Logi {
    /// `dev` は usage_page `FF00` / usage `0002` のコレクションを開いたもの。
    pub fn open(dev: hidapi::HidDevice, info: DeviceInfo) -> HubResult<Self> {
        let mut me = Self {
            dev: Mutex::new(dev),
            info: Mutex::new(info),
            dev_index: 0,
            name_id: 0,
            type_name_id: 0,
            dpi_id: 0,
            rate_id: 0,
            batt_id: 0,
            protocol: None,
            hidpp_name: None,
        };
        // レシーバーは 1 台目 = 1 のことがほとんど。応答するまで 1..=3 を試す。
        me.dev_index = me
            .detect_index()
            .ok_or_else(|| HubError::Disconnected)?;

        // 機能インデックスは起動ごとに変わる。無い機能は 0 のまま。
        me.name_id = me.feature(0x0003).unwrap_or(0);
        me.type_name_id = me.feature(0x0005).unwrap_or(0);
        me.dpi_id = me.feature(0x2201).unwrap_or(0);
        me.rate_id = me.feature(0x8060).unwrap_or(0);
        me.batt_id = me.feature(0x1004).unwrap_or(0);

        if me.dpi_id == 0 && me.batt_id == 0 && me.rate_id == 0 {
            return Err(HubError::Unsupported("HID++ 2.0 の設定機能"));
        }

        if let Ok(r) = me.ask::<2>(0x00, 0x01, &[]) {
            me.protocol = Some(format!("{}.{}", r[0], r[1]));
        }
        me.hidpp_name = me.device_name().ok().filter(|n| plausible_name(n));
        {
            let mut info = me.info.lock().unwrap_or_else(|p| p.into_inner());
            info.firmware = me.protocol.clone();
        }
        Ok(me)
    }

    /// デバイス自身が名乗る名前 (診断用)。
    pub fn hidpp_name(&self) -> Option<&str> {
        self.hidpp_name.as_deref()
    }

    fn detect_index(&self) -> Option<u8> {
        for i in 1..=3u8 {
            let mut w = [0u8; 20];
            w[0] = RID_LONG;
            w[1] = i;
            w[2] = 0x00;
            w[3] = 0x10 | SW_ID; // IRoot.getProtocolVersion
            {
                let dev = self.dev.lock().unwrap_or_else(|p| p.into_inner());
                if dev.write(&w).is_err() {
                    continue;
                }
            }
            let deadline = Instant::now() + Duration::from_millis(250);
            while Instant::now() < deadline {
                let mut buf = [0u8; 32];
                let got = {
                    let dev = self.dev.lock().unwrap_or_else(|p| p.into_inner());
                    dev.read_timeout(&mut buf, READ_SLICE_MS).unwrap_or(0)
                };
                if got < 4 || buf[0] != RID_LONG || buf[1] != i {
                    continue;
                }
                if buf[2] == 0x00 && (buf[3] >> 4) == 0x01 {
                    return Some(i);
                }
            }
        }
        None
    }

    /// 1 往復。応答の params 部分を返す。
    fn call(&self, feature: u8, func: u8, params: &[u8]) -> HubResult<Vec<u8>> {
        let mut last = HubError::Protocol("HID++: 応答なし".into());
        for _ in 0..2 {
            let mut w = [0u8; 20];
            w[0] = RID_LONG;
            w[1] = self.dev_index;
            w[2] = feature;
            w[3] = (func << 4) | SW_ID;
            let n = params.len().min(16);
            w[4..4 + n].copy_from_slice(&params[..n]);
            {
                let dev = self.dev.lock().unwrap_or_else(|p| p.into_inner());
                dev.write(&w)
                    .map_err(|e| HubError::Io(format!("HID++ write: {e}")))?;
            }
            let deadline = Instant::now() + REPLY_BUDGET;
            while Instant::now() < deadline {
                let mut buf = [0u8; 32];
                let got = {
                    let dev = self.dev.lock().unwrap_or_else(|p| p.into_inner());
                    dev.read_timeout(&mut buf, READ_SLICE_MS)
                        .map_err(|e| HubError::Io(format!("HID++ read: {e}")))?
                };
                if got < 4 || buf[0] != RID_LONG || buf[1] != self.dev_index {
                    continue;
                }
                if buf[2] == 0xFF {
                    // [rid, dev, 0xFF, feature, func|sw, errorCode]
                    return Err(HubError::Protocol(format!(
                        "HID++ error 0x{:02x} (feature 0x{:02x} fn {func})",
                        buf.get(5).copied().unwrap_or(0),
                        buf[3]
                    )));
                }
                if buf[2] == feature && (buf[3] >> 4) == func {
                    return Ok(buf[4..got].to_vec());
                }
            }
            last = HubError::Protocol(format!(
                "HID++: feature 0x{feature:02x} fn {func} の応答なし"
            ));
        }
        Err(last)
    }

    /// 応答の先頭 `N` byte を固定長で取り出す。
    fn ask<const N: usize>(&self, feature: u8, func: u8, params: &[u8]) -> HubResult<[u8; N]> {
        let r = self.call(feature, func, params)?;
        if r.len() < N {
            return Err(HubError::Protocol(format!(
                "HID++: 応答が短い ({} < {N})",
                r.len()
            )));
        }
        let mut out = [0u8; N];
        out.copy_from_slice(&r[..N]);
        Ok(out)
    }

    fn feature(&self, id: u16) -> Option<u8> {
        let r = self.call(0x00, 0x00, &id.to_be_bytes()).ok()?;
        let idx = *r.first()?;
        (idx != 0).then_some(idx)
    }

    fn device_name(&self) -> HubResult<String> {
        // 実測: この個体は `0x0003 IDeviceName` が壊れていて (呼ぶと関係ないバイト列が
        // 返る)、名前は `0x0005 IDeviceTypeAndName` の f0=長さ / f1=本体 から取れる。
        // 0x0005 が無い機種のために 0x0003 も順に試す。
        let mut last = HubError::Unsupported("機種名");
        for idx in [self.type_name_id, self.name_id] {
            if idx == 0 {
                continue;
            }
            match self.read_name_at(idx) {
                Ok(n) if plausible_name(&n) => return Ok(n),
                Ok(n) => last = HubError::Protocol(format!("機種名が不正: {n:?}")),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// 名前系機能 (0x0003 / 0x0005) の共通読み出し: f0 = 長さ、f1 = offset から 16 byte。
    fn read_name_at(&self, idx: u8) -> HubResult<String> {
        let len = self.ask::<1>(idx, 0x00, &[])?[0] as usize;
        if !(1..=64).contains(&len) {
            return Err(HubError::Protocol(format!("名前長 {len}")));
        }
        let mut out = Vec::with_capacity(len);
        let mut off = 0u8;
        while out.len() < len {
            let chunk = self.call(idx, 0x01, &[off])?;
            if chunk.is_empty() {
                break;
            }
            let take = chunk.len().min(len - out.len());
            out.extend_from_slice(&chunk[..take]);
            off = off.wrapping_add(take as u8);
        }
        Ok(String::from_utf8_lossy(&out).trim().to_string())
    }

    fn read_dpi(&self) -> HubResult<u32> {
        if self.dpi_id == 0 {
            return Err(HubError::Unsupported("DPI"));
        }
        let r = self.ask::<3>(self.dpi_id, 0x02, &[0])?;
        Ok(((r[1] as u32) << 8) | r[2] as u32)
    }

    fn max_dpi(&self) -> u32 {
        // `getSensorDpiList` のエンコードが未確定なので、機種既定の上限を使う。
        25_600
    }

    fn read_rate_index(&self) -> HubResult<u8> {
        if self.rate_id == 0 {
            return Err(HubError::Unsupported("レポートレート"));
        }
        Ok(self.ask::<1>(self.rate_id, 0x01, &[])?[0])
    }

    /// HID++ が返すレート (index, 対応表の Hz)。診断用。
    ///
    /// **実機の実測値とは一致しない** (オンボードプロファイル側のレートで動くため)。
    /// 例: index 1 を返すが Raw Input の実測は 1009 Hz。
    pub fn polling_rate(&self) -> Option<(u8, u16)> {
        let i = self.read_rate_index().ok()?;
        let hz = *RATE_TABLE.get(i as usize).unwrap_or(&0);
        Some((i, hz))
    }

    fn battery_now(&self) -> HubResult<Battery> {
        if self.batt_id == 0 {
            return Err(HubError::Unsupported("バッテリー"));
        }
        let r = self.ask::<2>(self.batt_id, 0x01, &[])?;
        // 実測: 33% 放電中で status=0x04。0/4 は放電、1..=3 は充電系として扱う。
        Ok(Battery {
            percent: Some(r[0].min(100)),
            charging: matches!(r[1], 1 | 2 | 3),
        })
    }
}

impl DeviceDriver for Logi {
    fn info(&self) -> DeviceInfo {
        self.info
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    fn caps(&self) -> Capabilities {
        let mut caps = Capabilities::empty();
        caps.battery = self.batt_id != 0;

        if self.rate_id != 0 {
            // HID++ の 0x8060 は「ホスト設定」で、実機はオンボードプロファイル側の
            // レートで動いている。実測 (Raw Input) は 1009 Hz なのに index は 1 を返す
            // ため、そのまま出すと誤解を招く。UI には出さず、実測ピーク表示に任せる。
            // (書き込みもファームが error 0x02 で拒否する — docs/LOGITECH_HIDPP.md)
        }

        if self.dpi_id != 0 {
            caps.mouse = Some(MouseCaps {
                dpi_min: 100,
                dpi_max: self.max_dpi(),
                dpi_step: 50,
                // DPI ステージはオンボードプロファイル (0x8100) 側。HID++ の DPI 機能は
                // 1 センサー 1 値なので、UI には 1 段だけ見せる。
                min_stages: 1,
                max_stages: 1,
                xy_split: false,
                stage_colors: false,
                stage_toggle: false,
                // 0x1B04 (ReprogrammableKeys) を持たないためボタン割当は出さない。
                buttons: vec![],
                button_kinds: vec![],
                require_left_click: false,
                macros: None,
                live_buttons: vec![],
                adaptive_dpi: None,
            });
        }
        caps
    }

    fn read(&self) -> HubResult<DeviceState> {
        let settings = BTreeMap::new();
        let mouse = if self.dpi_id != 0 {
            let dpi = self.read_dpi()?;
            Some(MouseState {
                stages: vec![DpiStage {
                    x: dpi,
                    y: None,
                    color: Rgb::default(),
                    enabled: true,
                }],
                active_stage: 0,
                buttons: vec![],
                macros: vec![],
                adaptive: None,
            })
        } else {
            None
        };
        Ok(DeviceState {
            profile: ProfileState::default(),
            settings,
            mouse,
            keyboard: None,
            lighting: BTreeMap::new(),
            peq: None,
        })
    }

    fn apply(&self, change: &Change) -> HubResult<()> {
        match change {
            Change::DpiStages { stages, active } => {
                if self.dpi_id == 0 {
                    return Err(HubError::Unsupported("DPI"));
                }
                let stage = stages
                    .get(*active)
                    .or_else(|| stages.first())
                    .ok_or_else(|| HubError::Invalid("DPI ステージが空".into()))?;
                let dpi = stage.x.clamp(100, self.max_dpi());
                self.call(self.dpi_id, 0x03, &[0, (dpi >> 8) as u8, dpi as u8])?;
                let now = self.read_dpi()?;
                if now != dpi {
                    return Err(HubError::Protocol(format!(
                        "DPI が反映されない (要求 {dpi}, 実際 {now})"
                    )));
                }
                Ok(())
            }
            _ => Err(HubError::Unsupported("この変更")),
        }
    }

    fn battery(&self) -> HubResult<Battery> {
        self.battery_now()
    }
}
