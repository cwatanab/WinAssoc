fn main() {
    slint_build::compile("src/ui/theme.slint").expect("theme.slint compile failed");
    slint_build::compile("src/ui/settings.slint").expect("settings.slint compile failed");
    slint_build::compile("src/picker/picker.slint").expect("picker.slint compile failed");
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.compile().expect("icon compile failed");
    }
}