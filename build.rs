fn main() {
    // slint_build::compile("src/settings/settings.slint").unwrap();
    // slint_build::compile("src/picker/picker.slint").unwrap();
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.compile().unwrap();
    }
}
