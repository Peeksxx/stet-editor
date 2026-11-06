fn main() {
    println!("cargo:rerun-if-changed=assets/stet.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/stet.ico")
            .set("ProductName", "STET")
            .set("FileDescription", "STET")
            .set("OriginalFilename", "stet.exe")
            .compile()
            .expect("Cannot compile STET resources");
    }
}
