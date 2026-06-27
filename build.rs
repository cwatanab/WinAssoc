fn main() {
    // Windows環境の場合、システムフォントから segmdl2.ttf を src/ui/ にコピーしてSlintがインポートできるようにする
    if cfg!(target_os = "windows") {
        let system_font = std::path::Path::new(r"C:\Windows\Fonts\segmdl2.ttf");
        let dest_font = std::path::Path::new("src/ui/segmdl2.ttf");
        if system_font.exists() {
            if let Err(e) = std::fs::copy(system_font, dest_font) {
                println!("cargo:warning=Failed to copy segmdl2.ttf: {}", e);
            }
        }
    }

    slint_build::compile("src/ui/settings.slint").expect("settings.slint compile failed");
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.compile().expect("icon compile failed");
    }
}
