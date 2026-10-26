fn main() {
    // Embeds the exe icon. Regenerate the .ico with `python assets/make_icon.py`.
    println!("cargo:rerun-if-changed=assets/gnomecuts.ico");
    embed_resource::compile("assets/gnomecuts.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
