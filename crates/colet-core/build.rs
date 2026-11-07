fn main() {
    let sdk = std::path::Path::new("vendor/everything-sdk3");
    if !sdk.join("src/Everything3.c").exists() {
        panic!("Everything SDK is missing at {}; run `git submodule update --init`", sdk.display());
    }
    cc::Build::new().file(sdk.join("src/Everything3.c")).include(sdk.join("include")).warnings(false).compile("everything3");
    println!("cargo:rerun-if-changed=vendor/everything-sdk3");
}
