import SwiftUI
import WebKit

/// Shows a message's HTML with no scripts, no storage and no network.
/// The page from the core also carries its own policy; a tapped link opens outside the app.
struct HTMLView: UIViewRepresentable {
    let html: String

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = .nonPersistent()
        config.defaultWebpagePreferences.allowsContentJavaScript = false
        let view = WKWebView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        Task {
            // Without the rule list the page's own policy still blocks the network.
            if let rules = await Self.rules.value {
                config.userContentController.add(rules)
            }
            view.loadHTMLString(html, baseURL: nil)
        }
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {}

    /// Blocks every load that is not the page itself or inline data. Compiled once.
    @MainActor private static let rules = Task<WKContentRuleList?, Never> {
        let json = #"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}},"#
            + #"{"trigger":{"url-filter":"^(about|data):"},"action":{"type":"ignore-previous-rules"}}]"#
        return await withCheckedContinuation { done in
            WKContentRuleListStore.default().compileContentRuleList(
                forIdentifier: "ozero-offline", encodedContentRuleList: json
            ) { list, _ in done.resume(returning: list) }
        }
    }

    @MainActor final class Coordinator: NSObject, WKNavigationDelegate {
        func webView(
            _ webView: WKWebView, decidePolicyFor action: WKNavigationAction
        ) async -> WKNavigationActionPolicy {
            switch action.navigationType {
            case .linkActivated:
                if let url = action.request.url, ["http", "https", "mailto"].contains(url.scheme?.lowercased()) {
                    await UIApplication.shared.open(url)
                }
                return .cancel
            case .other where action.request.url?.scheme == "about":
                return .allow
            default:
                return .cancel
            }
        }
    }
}
