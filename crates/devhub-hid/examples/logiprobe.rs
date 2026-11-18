//! Logitech HID++ 2.0 の実機ダンプ (DPI / レート / 電池 / 機種名)。
//!
//! ```sh
//! cargo run -p devhub-hid --example logiprobe
//! ```
//!
//! G HUB が起動していても読み取りは通る (書き込み検証のときは終了させる)。
//! 解析メモは `docs/LOGITECH_HIDPP.md`。

use devhub_core::backend::*;
use devhub_core::model::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = hidapi::HidApi::new()?;
    let mut found = false;
    for d in api.device_list() {
        if d.vendor_id() != 0x046D || d.usage_page() != 0xFF00 {
            continue;
        }
        println!(
            "vid={:04x} pid={:04x} usage_page={:04x} usage={:04x} if={} product={:?}",
            d.vendor_id(),
            d.product_id(),
            d.usage_page(),
            d.usage(),
            d.interface_number(),
            d.product_string()
        );
        // HID++ が通るのは usage 0002 のコレクションだけ (0001 は無応答)。
        if d.usage() != 0x0002 {
            continue;
        }
        found = true;
        let dev = api.open_path(d.path())?;
        let info = DeviceInfo {
            id: DeviceId(format!("hid:{:04x}:{:04x}:probe", d.vendor_id(), d.product_id())),
            kind: DeviceKind::Mouse,
            name: "PRO X SUPERLIGHT".into(),
            vendor: "Logicool".into(),
            model: "gpx-superlight".into(),
            vid: d.vendor_id(),
            pid: d.product_id(),
            transport: Transport::Wireless24G,
            firmware: None,
            dongle_firmware: None,
            serial: None,
            driver: DriverKind::Logi,
            battery: None,
            supported: true,
            asleep: false,
            board_id: None,
        };
        match devhub_hid::logi::open(dev, info) {
            Ok(drv) => {
                let info = drv.info();
                println!("  name     : {}", info.name);
                println!("  hidpp    : {:?}", drv.hidpp_name());
                println!("  firmware : {:?}", info.firmware);
                let caps = drv.caps();
                for s in &caps.settings {
                    println!("  setting  : {} ({:?})", s.id, s.kind);
                }
                match drv.read() {
                    Ok(state) => {
                        for (k, v) in &state.settings {
                            println!("  read     : {k} = {v:?}");
                        }
                        if let Some(m) = &state.mouse {
                            for st in &m.stages {
                                println!("  dpi      : {}", st.x);
                            }
                        }
                    }
                    Err(e) => println!("  read     : {e}"),
                }
                match drv.battery() {
                    Ok(b) => println!("  battery  : {:?}% charging={}", b.percent, b.charging),
                    Err(e) => println!("  battery  : {e}"),
                }
                println!("  rate     : {:?} (HID++ 値。実測とは一致しない)", drv.polling_rate());
            }
            Err(e) => println!("  open     : {e}"),
        }
    }
    if !found {
        println!("HID++ 2.0 (usage FF00/0002) のコレクションが見つかりません。");
    }
    Ok(())
}
