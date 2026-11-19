//! 実行ファイルへアイコンを埋め込む (build script から `#[path]` で読み込む)。
//!
//! GPUI 版の実行ファイルは、そのままだと `.rsrc` を持たない。すると
//! エクスプローラー・タスクバー・Alt+Tab・スタートメニューのショートカットが
//! すべて Windows の既定アイコンになる。窓のアイコンも同じ理由で欠ける
//! (`gpui-pre-windows` の `load_icon` は自分自身のリソース ID 1 を `LoadImageW` で読むので、
//! リソースが無いと `HICON::default()` に落ちて、タイトルバーとタスクバーが空になる)。
//!
//! そこで `rc.exe` を呼んで `assets/icon.ico` を ID 1 のアイコンにした `.res` を作り、
//! リンカへ渡す。Tauri 版 (`desktop/src-tauri/`) が埋めているのと同じ絵を使う。
//!
//! `rc.exe` は Windows SDK にしか無い。`RC` 環境変数 → `PATH` → SDK の既定の場所、の順に探し、
//! 最後まで見つからなければ警告だけ出して続ける (アイコンは付かないがビルドは通る)。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `icon` はパッケージ直下からの相対パス (例: `assets/icon.ico`)。
pub fn embed(icon: &str) {
    println!("cargo:rerun-if-changed={icon}");
    println!("cargo:rerun-if-env-changed=RC");
    // Windows 以外に出すことは今のところ無いが、`rc.exe` も無いので素直に諦める。
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let source = manifest.join(icon);
    if !source.is_file() {
        println!(
            "cargo:warning=アイコンが見つかりません: {}",
            source.display()
        );
        return;
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    // Windows SDK の `rc.exe` が第一候補。無ければ LLVM の `llvm-rc.exe` (SDK 無しで
    // VERSIONINFO まで通る)。どちらも無ければ `.res` を自前で組む。
    let Some(rc) = find_rc().or_else(find_llvm_rc) else {
        // Windows SDK も LLVM も無い環境でもアイコンだけは付くように、`.res` を
        // 自前で組み立てる。VERSIONINFO は付けない (rc.exe があるときだけ入る) が、
        // アイコン・タスクバー・Alt+Tab はこれで揃う。
        match write_res(&source, &out_dir) {
            Some(resource) => {
                println!(
                    "cargo:warning=rc.exe / llvm-rc が無いためアイコンのみ埋め込みます (バージョン情報なし)"
                );
                println!("cargo:rustc-link-arg-bins={}", resource.display());
            }
            None => println!(
                "cargo:warning=rc.exe も llvm-rc も無く、{} も読めないためアイコンは付きません。",
                source.display()
            ),
        }
        return;
    };
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    // `.rc` は ASCII で書く。アイコンの在り処は `/i` (コマンドライン経由) で渡すので、
    // 日本語を含むパス (この作業ディレクトリ) でも rc.exe の文字コードに左右されない。
    let script = out.join("app-icon.rc");
    let resource = out.join("app-icon.res");
    let name = source.file_name().expect("icon file name");
    // バージョン情報 (VERSIONINFO) も同じ .rc で埋め込む。
    // FileDescription がタスクマネージャー/ファイルプロパティの表示名になる。
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut vparts = version.split('.').filter_map(|p| p.parse::<u32>().ok());
    let fv = format!(
        "{},{},{},0",
        vparts.next().unwrap_or(0),
        vparts.next().unwrap_or(0),
        vparts.next().unwrap_or(0)
    );
    fs::write(
        &script,
        format!(
            concat!(
                "1 ICON \"{name}\"\r\n",
                "1 VERSIONINFO\r\n",
                "FILEVERSION {fv}\r\n",
                "PRODUCTVERSION {fv}\r\n",
                "FILEFLAGSMASK 0x3fL\r\n",
                "FILEFLAGS 0x0L\r\n",
                "FILEOS 0x40004L\r\n",
                "FILETYPE 0x1L\r\n",
                "FILESUBTYPE 0x0L\r\n",
                "BEGIN\r\n",
                "    BLOCK \"StringFileInfo\"\r\n",
                "    BEGIN\r\n",
                "        BLOCK \"040904b0\"\r\n",
                "        BEGIN\r\n",
                "            VALUE \"CompanyName\", \"Device HUB\"\r\n",
                "            VALUE \"FileDescription\", \"Device HUB\"\r\n",
                "            VALUE \"FileVersion\", \"{ver}\"\r\n",
                "            VALUE \"InternalName\", \"device-hub\"\r\n",
                "            VALUE \"OriginalFilename\", \"device-hub.exe\"\r\n",
                "            VALUE \"ProductName\", \"Device HUB\"\r\n",
                "            VALUE \"ProductVersion\", \"{ver}\"\r\n",
                "        END\r\n",
                "    END\r\n",
                "    BLOCK \"VarFileInfo\"\r\n",
                "    BEGIN\r\n",
                "        VALUE \"Translation\", 0x409, 1200\r\n",
                "    END\r\n",
                "END\r\n"
            ),
            name = name.to_string_lossy(),
            fv = fv,
            ver = version,
        ),
    )
    .expect("write app-icon.rc");
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/I")
        .arg(source.parent().expect("icon directory"))
        .arg("/fo")
        .arg(&resource)
        .arg(&script)
        .current_dir(&manifest)
        .status();
    match status {
        Ok(status) if status.success() => {
            // リンカへの入力として渡す。`link.exe` は `.res` をそのまま飲み込む。
            println!("cargo:rustc-link-arg-bins={}", resource.display());
        }
        Ok(status) => panic!(
            "リソースコンパイラが失敗しました (終了コード {:?}): {}",
            status.code(),
            rc.display()
        ),
        Err(error) => panic!("リソースコンパイラを実行できません ({}): {error}", rc.display()),
    }
}

/// `llvm-rc` (LLVM 同梱のリソースコンパイラ)。`rc.exe` と同じ `.rc` を受け付ける。
fn find_llvm_rc() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("LLVM_RC").map(PathBuf::from) {
        if explicit.is_file() {
            return Some(explicit);
        }
    }
    let path = env::var_os("PATH")?;
    let mut found = env::split_paths(&path)
        .map(|dir| dir.join("llvm-rc.exe"))
        .find(|candidate| candidate.is_file());
    if found.is_none() {
        // LLVM を PATH に入れていない環境向け (winget / 手動インストールの既定の場所)。
        let roots = ["ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(env::var_os)
            .map(PathBuf::from);
        for root in roots {
            let llvm = root.join("LLVM").join("bin").join("llvm-rc.exe");
            if llvm.is_file() {
                found = Some(llvm);
                break;
            }
        }
    }
    found
}

/// `RC` → `PATH` → Windows SDK の既定の場所 (`%ProgramFiles(x86)%\Windows Kits\10\bin\<版>\<arch>`)。
fn find_rc() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("RC").map(PathBuf::from) {
        if explicit.is_file() {
            return Some(explicit);
        }
    }
    if let Some(path) = env::var_os("PATH") {
        if let Some(found) = env::split_paths(&path)
            .map(|dir| dir.join("rc.exe"))
            .find(|c| c.is_file())
        {
            return Some(found);
        }
    }
    // ビルド環境によっては `WindowsSdkVerBinPath` が直接刺さっている (VS の開発者プロンプト)。
    if let Some(bin) = env::var_os("WindowsSdkVerBinPath").map(PathBuf::from) {
        for arch in arch_names() {
            let candidate = bin.join(arch).join("rc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let roots = ["ProgramFiles(x86)", "ProgramFiles"]
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from);
    for root in roots {
        let bin = root.join("Windows Kits").join("10").join("bin");
        let Ok(entries) = fs::read_dir(&bin) else {
            continue;
        };
        let mut versions: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        // SDK は複数入っていることがある。版が新しいほうを採る (文字列比較だと 10.0.9 が 10.0.26 より後ろに来る)。
        versions.sort_by_key(|a| version_key(a));
        for version in versions.into_iter().rev() {
            for arch in arch_names() {
                let candidate = version.join(arch).join("rc.exe");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// 目標の CPU に対応する SDK の bin の名前。x64 を先に見る (x64 の rc.exe はどの目標でも使える)。
fn arch_names() -> &'static [&'static str] {
    match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => &["arm64", "x64"],
        Ok("x86") => &["x86", "x64"],
        _ => &["x64"],
    }
}

fn version_key(path: &Path) -> Vec<u32> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name.split('.')
                .map(|part| part.parse().unwrap_or(0))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------- .res 直書き

/// `RT_ICON` / `RT_GROUP_ICON` だけを持つ `.res` を作る (rc.exe 無しのフォールバック)。
///
/// `.res` は「エントリの並び」で、各エントリは
/// `DataSize, HeaderSize, type, name, DataVersion, MemoryFlags, LangId, Version,
/// Characteristics, data(4 byte 境界までパディング)`。
/// type/name が序数なら `0xFFFF, id` の 4 byte。
fn write_res(ico: &Path, out: &Path) -> Option<PathBuf> {
    let ico = fs::read(ico).ok()?;
    let images = parse_ico(&ico)?;

    let mut res = Vec::new();
    // `.res` は「ヌル リソース」(type/name 0 で中身なし) で始まる 32 byte のヘッダを持つ。
    push_res(&mut res, 0, 0, &[]);
    // RT_ICON (3): 1 枚ずつ、名前は 1 始まりの序数。
    for (index, image) in images.iter().enumerate() {
        let origin = &ico[image.offset..image.offset + image.size];
        push_res(&mut res, 3, (index + 1) as u16, origin);
    }
    // RT_GROUP_ICON (14): どの序数がどのサイズかを並べた目次。
    let mut group = Vec::with_capacity(6 + images.len() * 14);
    group.extend_from_slice(&0u16.to_le_bytes()); // reserved
    group.extend_from_slice(&1u16.to_le_bytes()); // 1 = icon
    group.extend_from_slice(&(images.len() as u16).to_le_bytes());
    for (index, image) in images.iter().enumerate() {
        group.push(image.width);
        group.push(image.height);
        group.push(image.colors);
        group.push(0);
        group.extend_from_slice(&image.planes.to_le_bytes());
        group.extend_from_slice(&image.bits.to_le_bytes());
        group.extend_from_slice(&image.size.to_le_bytes());
        group.extend_from_slice(&((index + 1) as u16).to_le_bytes());
    }
    push_res(&mut res, 14, 1, &group);

    let path = out.join("app-icon.res");
    fs::write(&path, res).ok()?;
    Some(path)
}

struct IcoImage {
    width: u8,
    height: u8,
    colors: u8,
    planes: u16,
    bits: u16,
    size: usize,
    offset: usize,
}

/// ICONDIR + ICONDIRENTRY を読む。データは `ico` 内のオフセットで持つ。
fn parse_ico(ico: &[u8]) -> Option<Vec<IcoImage>> {
    if ico.len() < 6 {
        return None;
    }
    let reserved = u16::from_le_bytes([ico[0], ico[1]]);
    let kind = u16::from_le_bytes([ico[2], ico[3]]);
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    if reserved != 0 || kind != 1 || count == 0 || ico.len() < 6 + count * 16 {
        return None;
    }
    let mut images = Vec::with_capacity(count);
    for i in 0..count {
        let e = &ico[6 + i * 16..6 + i * 16 + 16];
        let size = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        let offset = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
        if size == 0 || offset.checked_add(size)? > ico.len() {
            return None;
        }
        images.push(IcoImage {
            width: e[0],
            height: e[1],
            colors: e[2],
            planes: u16::from_le_bytes([e[4], e[5]]),
            bits: u16::from_le_bytes([e[6], e[7]]),
            size,
            offset,
        });
    }
    Some(images)
}

/// `.res` のエントリ 1 つを追加する。type/name は序数のみ (アイコンは序数で足りる)。
///
/// `HeaderSize` は `DataSize` / `HeaderSize` 自身も含めた「データ前までの長さ」で、
/// 序数 type + 序数 name の場合は 32。
fn push_res(res: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    let header_size: u32 = 8 + 4 + 4 + 4 + 2 + 2 + 4 + 4;
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    res.extend_from_slice(&header_size.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&kind.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&id.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    res.extend_from_slice(&0x1010u16.to_le_bytes()); // MOVEABLE | DISCARDABLE
    res.extend_from_slice(&0x0409u16.to_le_bytes()); // en-US
    res.extend_from_slice(&0u32.to_le_bytes()); // Version
    res.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    res.extend_from_slice(data);
    while res.len() % 4 != 0 {
        res.push(0);
    }
}
