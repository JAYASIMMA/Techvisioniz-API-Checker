fn main() {
    println!("cargo:rerun-if-changed=assets/techvision-api-checker.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/techvision-api-checker.ico");
        res.set("ProductName", "Techvision API Checker");
        res.set("FileDescription", "Techvision API Checker");
        res.set("CompanyName", "Techvision");
        res.set("LegalCopyright", "Copyright © Techvision");
        res.set("InternalName", "techvision-api-checker");
        res.set("OriginalFilename", "techvision-api-checker.exe");
        if let Err(error) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {error}");
        }
    }
}
