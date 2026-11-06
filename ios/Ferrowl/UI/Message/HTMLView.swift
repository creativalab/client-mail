import SwiftUI
import WebKit

/// HTML mail in a locked-down WKWebView: no JavaScript, a strict
/// Content-Security-Policy (no scripts, frames, forms or fonts from the
/// network), remote images only when allowed. `cid:` images come from the
/// message itself. Links open in the browser; `mailto:` writes a message.
struct HTMLView: UIViewRepresentable {
    let html: String
    let inline: [String: Data]
    let remoteImages: Bool
    @Binding var height: CGFloat
    let openMailto: (URL) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.defaultWebpagePreferences.allowsContentJavaScript = false
        config.websiteDataStore = .nonPersistent()
        config.dataDetectorTypes = [.link, .phoneNumber]
        config.suppressesIncrementalRendering = true
        let view = WKWebView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        // Links with target="_blank" ask for a new window: opened outside too.
        view.uiDelegate = context.coordinator
        view.isOpaque = false
        view.backgroundColor = .clear
        view.scrollView.isScrollEnabled = false
        view.scrollView.bounces = false
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {
        context.coordinator.parent = self
        let page = Self.page(html, inline: inline, remoteImages: remoteImages)
        guard context.coordinator.loaded != page else { return }
        context.coordinator.loaded = page
        view.loadHTMLString(page, baseURL: nil)
    }

    /// The mail inside a page that fits the phone's width (wider layouts are
    /// scaled down) with the privacy rules above.
    static func page(_ html: String, inline: [String: Data], remoteImages: Bool) -> String {
        var body = html
        // cid: images become data: URLs.
        for (cid, data) in inline {
            let mime = Self.mime(of: data)
            let uri = "data:\(mime);base64,\(data.base64EncodedString())"
            body = body.replacingOccurrences(of: "cid:\(cid)", with: uri)
        }
        // The page sets its own viewport.
        body = body.replacingOccurrences(
            of: #"<meta[^>]*name\s*=\s*["']?viewport[^>]*>"#, with: "",
            options: [.regularExpression, .caseInsensitive]
        )
        let images = remoteImages ? "data: https: http:" : "data:"
        let csp = "default-src 'none'; img-src \(images); style-src 'unsafe-inline' data:; media-src data:; font-src data:"
        let head = """
        <meta charset="utf-8">
        <meta http-equiv="Content-Security-Policy" content="\(csp)">
        <meta name="viewport" content="width=device-width, initial-scale=1, shrink-to-fit=yes">
        <meta name="color-scheme" content="light only">
        <style>
          html { -webkit-text-size-adjust: 100%; }
          body { margin: 0; padding: 0 2px; font: -apple-system-body; word-wrap: break-word; background: #fff; color: #1F1B18; }
          img, video { max-width: 100% !important; height: auto !important; }
          pre { white-space: pre-wrap; }
          table { max-width: 100%; }
        </style>
        """
        if body.range(of: "<head", options: .caseInsensitive) != nil {
            return body.replacingOccurrences(of: "<head>", with: "<head>\(head)", options: .caseInsensitive)
                .replacingOccurrences(of: #"<head\s([^>]*)>"#, with: "<head $1>\(head)", options: [.regularExpression, .caseInsensitive])
        }
        return "<!doctype html><html><head>\(head)</head><body>\(body)</body></html>"
    }

    private static func mime(of data: Data) -> String {
        let b = [UInt8](data.prefix(4))
        if b.starts(with: [0x89, 0x50]) { return "image/png" }
        if b.starts(with: [0xFF, 0xD8]) { return "image/jpeg" }
        if b.starts(with: [0x47, 0x49]) { return "image/gif" }
        if b.count == 4, b == [0x52, 0x49, 0x46, 0x46] { return "image/webp" }
        return "application/octet-stream"
    }

    final class Coordinator: NSObject, WKNavigationDelegate, WKUIDelegate {
        var parent: HTMLView
        var loaded: String?

        init(_ parent: HTMLView) { self.parent = parent }

        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                     decisionHandler: @escaping @MainActor (WKNavigationActionPolicy) -> Void) {
            guard let url = action.request.url else { return decisionHandler(.cancel) }
            // The page itself (loadHTMLString) and its data: images load here;
            // everything else is a link the user followed.
            let scheme = url.scheme?.lowercased()
            if scheme == "about" || scheme == "data" || (action.navigationType == .other && action.targetFrame?.isMainFrame == true && scheme == nil) {
                return decisionHandler(.allow)
            }
            decisionHandler(.cancel)
            // Only what the user tapped: never redirects the mail makes by itself.
            if action.navigationType == .linkActivated { open(url) }
        }

        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                     for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
            if action.navigationType == .linkActivated, let url = action.request.url { open(url) }
            return nil
        }

        private func open(_ url: URL) {
            switch url.scheme?.lowercased() {
            case "mailto": parent.openMailto(url)
            case "http", "https", "tel": UIApplication.shared.open(url)
            default: break
            }
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            // Height of the content, so the message scrolls as one page.
            webView.evaluateJavaScript("document.documentElement.scrollHeight") { [weak self] value, _ in
                guard let h = value as? CGFloat else { return }
                DispatchQueue.main.async { self?.parent.height = max(h, 40) }
            }
        }
    }
}
