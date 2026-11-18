//! 実機で DPI を往復変更する (アプリと同じ `scan` → `driver` → `apply` 経路)。
//! ```sh
//! cargo run -p devhub-hid --example dpiwrite
//! cargo run -p devhub-hid --example dpiwrite -- 800
//! ```
//!
//! UI の DPI 変更は `Change::DpiStages` を `DeviceDriver::apply` に渡すだけなので、
//! この example が通れば UI からの変更も通る (クリック以外の経路が同一)。

use devhub_core::model::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target: u32 = match std::env::args().nth(1) {
        Some(s) => s.parse()?,
        None => 800,
    };

    let hub = devhub_hid::connect()?;
    let devices = hub.scan()?;
    let mouse = devices
        .iter()
        .find(|d| d.driver == DriverKind::Logi)
        .ok_or("Logitech (HID++) のデバイスが見つかりません")?;
    println!("device : {} ({:?}) id={}", mouse.name, mouse.driver, mouse.id);

    let drv = hub.driver(&mouse.id)?;

    let before = drv.read()?;
    let (orig, stages) = match &before.mouse {
        Some(m) => (
            m.stages.get(m.active_stage).map(|s| s.x).unwrap_or(0),
            m.stages.clone(),
        ),
        None => return Err("mouse state がありません".into()),
    };
    println!("before : {orig} dpi ({} stages)", stages.len());

    // 800 dpi を用意 (元の値を末尾に残して必ず戻せるようにする)
    let mut next: Vec<DpiStage> = stages.clone();
    let idx = before.mouse.as_ref().map(|m| m.active_stage).unwrap_or(0);
    if next.is_empty() {
        next.push(DpiStage {
            x: target,
            y: None,
            color: Rgb::default(),
            enabled: true,
        });
    } else {
        next[idx].x = target;
        next[idx].y = None;
    }

    drv.apply(&Change::DpiStages {
        stages: next,
        active: idx,
    })?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    let mid = drv
        .read()?
        .mouse
        .and_then(|m| m.stages.get(m.active_stage).map(|s| s.x))
        .unwrap_or(0);
    println!("apply  : {target} -> read {mid}");

    // 元に戻す
    drv.apply(&Change::DpiStages {
        stages: stages.clone(),
        active: idx,
    })?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    let after = drv
        .read()?
        .mouse
        .and_then(|m| m.stages.get(m.active_stage).map(|s| s.x))
        .unwrap_or(0);
    println!("restore: {orig} -> read {after}");

    if mid != target {
        return Err(format!("書き込み後の読み値が {mid} で不一致").into());
    }
    if after != orig {
        return Err(format!("復帰後の読み値が {after} で不一致").into());
    }
    println!("OK: DPI 往復成功 (apply 経路 = UI と同じ)");
    Ok(())
}
