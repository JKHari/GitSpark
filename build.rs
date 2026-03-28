#[cfg(windows)]
fn main() {
    println!("cargo:rerun-if-changed=assets/gitspark.jpg");
    println!("cargo:rerun-if-changed=assets/gitspark.ico");
    println!("cargo:rerun-if-changed=windows/app.rc");
    let _ = embed_resource::compile("windows/app.rc", embed_resource::NONE);
}

#[cfg(not(windows))]
fn main() {}
