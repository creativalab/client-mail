fn main() {
    // Embed the application icon in the Windows executable.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icons/ferrowl.ico");
        res.compile().expect("failed to embed the Windows icon");
    }
}
