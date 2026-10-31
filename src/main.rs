mod app;
mod config;
mod mail;
mod worker;

fn main() -> eframe::Result {
    // lettre usa il provider crittografico di default di rustls: lo fissiamo a ring.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Client Mail")
            .with_inner_size([1200.0, 760.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Client Mail",
        options,
        Box::new(|cc| Ok(Box::new(app::MailApp::new(cc)))),
    )
}
