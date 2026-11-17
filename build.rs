// Windows: bädda in R-ikonen i ruppa.exe (syns i Utforskaren, aktivitetsfältet och Start).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon/ruppa.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/ruppa.ico");
        res.set("ProductName", "Ruppa");
        res.set("FileDescription", "Ruppa – Uppa, Puppa och Muppa i ett fönster");
        if let Err(e) = res.compile() {
            println!("cargo:warning=kunde inte bädda in ikonen: {e}");
        }
    }
}
