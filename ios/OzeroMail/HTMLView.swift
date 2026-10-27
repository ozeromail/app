import SwiftUI
import WebKit

/// Shows a message's HTML with no scripts, no storage and no network, as tall as its page.
/// The page from the core also carries its own policy; a tapped link opens outside the app.
/// The page asks for its loaded `images` by address; they go in as inline data, so the page
/// needs nothing from outside.
struct HTMLView: View {
    let html: String
    var images: [RemoteImage?] = []
    var message: Int64 = 0

    @State private var height: CGFloat = 96

    var body: some View {
        Page(html: html, images: images, message: message, height: $height)
            .frame(height: height)
    }
}

private struct Page: UIViewRepresentable {
    let html: String
    let images: [RemoteImage?]
    let message: Int64
    @Binding var height: CGFloat

    /// The page with its image addresses, `https://images.ozero.invalid/<message>/<n>`, made
    /// inline data, and the reading font left to the system's serif.
    private func inline(_ html: String) -> String {
        guard !images.isEmpty,
              let address = try? Regex("https://images\\.ozero\\.invalid/\(message)/([0-9]+)")
        else { return html }
        // The whole number each time, so image 1 never takes part of image 12's address.
        return html.replacing(address) { match in
            guard let n = Int(match.output[1].substring ?? ""), n < images.count, let image = images[n] else {
                return String(match.output[0].substring ?? "")
            }
            return "data:\(image.mime);base64,\(image.data.base64EncodedString())"
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(height: $height) }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.websiteDataStore = Self.store
        config.defaultWebpagePreferences.allowsContentJavaScript = false
        config.dataDetectorTypes = [.link, .phoneNumber]
        let view = WKWebView(frame: .zero, configuration: config)
        view.navigationDelegate = context.coordinator
        // A plain page is clear and takes the screen's colour; a newsletter paints its own.
        view.isOpaque = false
        view.backgroundColor = .clear
        view.scrollView.backgroundColor = .clear
        // As tall as its page, it scrolls with the conversation instead.
        view.scrollView.isScrollEnabled = false
        context.coordinator.watch(view.scrollView)
        Task {
            // Without the rule list the page's own policy still blocks the network.
            if let rules = await Self.rules.value {
                view.configuration.userContentController.add(rules)
            }
            // Once per web view: it loads its page only once.
            view.loadHTMLString(inline(html), baseURL: nil)
        }
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {}

    /// One store for every message: nothing is kept on disk, and the views can share a process.
    @MainActor private static let store = WKWebsiteDataStore.nonPersistent()

    /// Blocks every load that is not the page itself or inline data. Compiled once, then read from disk.
    @MainActor private static let rules = Task<WKContentRuleList?, Never> {
        // A new rule needs a new id, or the old compiled list is read back.
        let id = "ozero-offline-1"
        let json = #"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}},"#
            + #"{"trigger":{"url-filter":"^(about|data):"},"action":{"type":"ignore-previous-rules"}}]"#
        let found: WKContentRuleListStore? = WKContentRuleListStore.default()
        guard let lists = found else { return nil }
        let saved = await withCheckedContinuation { done in
            lists.lookUpContentRuleList(forIdentifier: id) { list, _ in done.resume(returning: list) }
        }
        if let saved { return saved }
        return await withCheckedContinuation { done in
            lists.compileContentRuleList(forIdentifier: id, encodedContentRuleList: json) { list, _ in
                done.resume(returning: list)
            }
        }
    }

    @MainActor final class Coordinator: NSObject, WKNavigationDelegate {
        private let height: Binding<CGFloat>
        private var watching: NSKeyValueObservation?

        init(height: Binding<CGFloat>) {
            self.height = height
        }

        /// Follows the page's height as it lays out.
        func watch(_ scroll: UIScrollView) {
            watching = scroll.observe(\.contentSize, options: [.new]) { [height] _, change in
                guard let size = change.newValue, size.height > 0 else { return }
                Task { @MainActor in
                    if abs(height.wrappedValue - size.height) > 1 { height.wrappedValue = size.height }
                }
            }
        }

        func webView(
            _ webView: WKWebView, decidePolicyFor action: WKNavigationAction
        ) async -> WKNavigationActionPolicy {
            switch action.navigationType {
            case .linkActivated where action.request.url?.scheme == "about":
                // A link to a part of the same page.
                return .allow
            case .linkActivated:
                if let url = action.request.url, ["http", "https", "mailto", "tel"].contains(url.scheme?.lowercased()) {
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
