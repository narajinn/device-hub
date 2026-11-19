//! `0x1004` (UnifiedBattery) の生フレーム ダンプ。
//!
//! 充電状態の判定に使うバイトを実機で確認するための診断用。
//!
//! ```sh
//! cargo run --release -p devhub-hid --example batdump
//! ```

use std::time::Duration;

fn ask(dev: &hidapi::HidDevice, feature: u8, func: u8, params: &[u8]) -> Option<Vec<u8>> {
    let mut out = [0u8; 20];
    out[0] = 0x11;
    out[1] = 0x01;
    out[2] = feature;
    out[3] = (func << 4) | 0x0A;
    out[4..4 + params.len()].copy_from_slice(params);
    dev.write(&out).ok()?;
    let mut buf = [0u8; 32];
    let n = dev.read_timeout(&mut buf, 700).ok()?;
    Some(buf[..n].to_vec())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = hidapi::HidApi::new()?;
    let target = api
        .device_list()
        .find(|d| {
            d.vendor_id() == 0x046D && d.usage_page() == 0xFF00 && d.usage() == 0x0002
        })
        .map(|d| d.path().to_owned());
    let Some(path) = target else {
        println!("HID++ 2.0 (usage FF00/0002) のコレクションが見つかりません。");
        return Ok(());
    };
    let dev = api.open_path(&path)?;

    // IRoot (0x0000) getFeature → 0x1004 のインデックス。
    let reply = ask(&dev, 0x00, 0, &[0x10, 0x04]).expect("getFeature reply");
    println!("getFeature(0x1004): {}", hex(&reply));
    let batt = reply[4];
    if batt == 0 {
        println!("0x1004 は非搭載です。");
        return Ok(());
    }

    println!("--- f0 getCapabilities ---");
    if let Some(r) = ask(&dev, batt, 0, &[]) {
        println!("{}", hex(&r));
    }

    println!("--- f1 getStatus (5 秒おきに 6 回) ---");
    for i in 0..6 {
        match ask(&dev, batt, 1, &[]) {
            Some(r) => println!(
                "t={}s  {}   percent={} level=0x{:02x} status=0x{:02x}",
                i * 5,
                hex(&r),
                r.get(4).copied().unwrap_or(0),
                r.get(5).copied().unwrap_or(0),
                r.get(6).copied().unwrap_or(0)
            ),
            None => println!("t={}s  <timeout>", i * 5),
        }
        if i < 5 {
            std::thread::sleep(Duration::from_secs(5));
        }
    }

    println!("--- f2 (存在すれば) ---");
    if let Some(r) = ask(&dev, batt, 2, &[]) {
        println!("{}", hex(&r));
    }
    Ok(())
}
