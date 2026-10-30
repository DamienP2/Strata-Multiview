// Embeds the application icon into the Windows executable.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icons/strata.ico");
        res.set("FileDescription", "Strata Multiview");
        res.set("ProductName", "Strata Multiview");
        if let Err(e) = res.compile() {
            println!("cargo:warning=cannot embed the icon: {e}");
        }
    }
    println!("cargo:rerun-if-changed=../../assets/icons/strata.ico");
}
