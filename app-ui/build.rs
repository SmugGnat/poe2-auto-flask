fn main() {
    println!("cargo:rerun-if-changed=../Cargo.toml");
    let manifest: toml::Value = std::fs::read_to_string("../Cargo.toml")
        .expect("read application version")
        .parse()
        .expect("parse root Cargo manifest");
    let version = manifest["package"]["version"]
        .as_str()
        .expect("application version");
    // The click-level UI regression tests inspect and click actual Slint
    // elements, which requires compiler debug metadata. Keep it out of the
    // shipped release binary to preserve its small size.
    let mut configuration =
        slint_build::CompilerConfiguration::new().with_style("fluent-dark".into());
    if std::env::var("PROFILE").as_deref() != Ok("release") {
        configuration = configuration.with_debug_info(true);
    }
    slint_build::compile_with_config("ui/main.slint", configuration)
        .expect("production Slint UI must compile");

    // Window.icon controls Slint's runtime icon, but the Windows shell
    // also reads the icon resource embedded in the PE executable.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=ui/icon.ico");
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("ui/icon.ico");
        resource.set("ProductName", "PoE Auto Flask");
        resource.set("FileDescription", "PoE Auto Flask");
        resource.set("ProductVersion", version);
        resource.set("FileVersion", version);
        let numeric_version = version
            .split('.')
            .try_fold(0u64, |value, part| {
                part.parse::<u16>()
                    .map(|part| (value << 16) | u64::from(part))
            })
            .expect("numeric application version")
            << 16;
        resource.set_version_info(winresource::VersionInfo::FILEVERSION, numeric_version);
        resource.set_version_info(winresource::VersionInfo::PRODUCTVERSION, numeric_version);
        resource
            .compile()
            .expect("embed Auto Flask executable icon");
    }
}
