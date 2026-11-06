import AuthenticationServices
import UIKit

/// Sign in with Microsoft or Google on iOS.
///
/// The page opens in an `ASWebAuthenticationSession` (Safari's sign-in
/// sheet, sharing no cookies with Safari); the provider then sends it to
/// the app's own address:
/// - Microsoft: `msauth.com.ferrowl.app://auth`, registered as a redirect
///   on the Ferrowl app in Microsoft Entra;
/// - Google: the "Ferrowl iOS" client id reversed (see `oauth.rs`).
///
/// The engine checks the answer, exchanges the code (PKCE) and keeps the
/// tokens in the Keychain ([KeychainTokenVault]).
@MainActor
final class OAuthSignIn: NSObject, ASWebAuthenticationPresentationContextProviding {
    private static func redirect(_ provider: OAuthProvider) -> String {
        switch provider {
        case .microsoft: "msauth.com.ferrowl.app://auth"
        case .google: googleIosRedirect()
        }
    }

    private var session: ASWebAuthenticationSession?

    /// Signs in and returns the signed-in address; nil when the user closed
    /// the sheet.
    func signIn(_ provider: OAuthProvider, email: String) async throws -> String? {
        let redirect = Self.redirect(provider)
        let scheme = String(redirect.prefix { $0 != ":" })
        let request = try oauthBegin(provider: provider, loginHint: email, redirect: redirect)
        guard let url = URL(string: request.url) else { return nil }
        let answer: URL? = try await withCheckedThrowingContinuation { continuation in
            let session = ASWebAuthenticationSession(url: url, callbackURLScheme: scheme) { url, error in
                if let url {
                    continuation.resume(returning: url)
                } else if let error = error as? ASWebAuthenticationSessionError, error.code == .canceledLogin {
                    continuation.resume(returning: nil)
                } else {
                    continuation.resume(throwing: error ?? FerrowlError.Failed(message: t("err_generic")))
                }
            }
            session.presentationContextProvider = self
            session.prefersEphemeralWebBrowserSession = true
            self.session = session
            session.start()
        }
        session = nil
        guard let answer else { return nil }
        return try await oauthFinish(provider: provider, request: request, answer: answer.absoluteString, loginHint: email)
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .first(where: \.isKeyWindow) ?? ASPresentationAnchor()
        }
    }
}
