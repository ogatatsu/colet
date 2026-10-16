fn main() {
    cc::Build::new()
        .file("vendor/everything-sdk3/src/Everything3.c")
        .include("vendor/everything-sdk3/include")
        .warnings(false)
        .compile("everything3");
    println!("cargo:rerun-if-changed=vendor/everything-sdk3");
}
