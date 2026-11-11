fn main() {
    // Colet's icon, as resource 1, which GPUI shows in the title bar and the taskbar.
    embed_resource::compile("resources/colet.rc", embed_resource::NONE).manifest_optional().unwrap();
    println!("cargo:rerun-if-changed=resources");
}
