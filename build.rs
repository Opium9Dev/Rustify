fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    println!("cargo:rerun-if-changed=assets/app.ico");

    let mut resource = winres::WindowsResource::new();
    resource.set_icon("assets/app.ico");
    if let Err(error) = resource.compile() {
        panic!("Failed to embed assets/app.ico: {error}");
    }
}
