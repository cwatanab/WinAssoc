fn main() {
    let picker_slint = "src/picker/picker.slint";
    let settings_slint = "src/settings/settings.slint";
    if std::path::Path::new(picker_slint).exists() {
        slint_build::compile(picker_slint).unwrap();
    }
    if std::path::Path::new(settings_slint).exists() {
        slint_build::compile(settings_slint).unwrap();
    }
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.compile().unwrap();
    }
}
