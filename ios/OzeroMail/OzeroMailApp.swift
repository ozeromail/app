import SwiftUI

@main
struct OzeroMailApp: App {
    var body: some Scene {
        WindowGroup {
            RootView().tint(.ozeroAccent)
        }
        .backgroundTask(.appRefresh(BackgroundCheck.id)) { await BackgroundCheck.run() }
    }
}

private enum Start {
    case loading
    case signIn(Mail)
    case inbox(Mail)
    case failed(LocalizedStringKey)
}

private struct RootView: View {
    @State private var start = Start.loading

    var body: some View {
        Group {
            switch start {
            case .loading:
                Color.clear
            case .signIn(let mail):
                SignInView(mail: mail) { start = .inbox(mail) }
            case .inbox(let mail):
                InboxView(mail: mail)
            case .failed(let text):
                Text(text).padding(24)
            }
        }
        .task {
            do {
                let mail = try await Mail.shared()
                start = try await mail.hasAccounts() ? .inbox(mail) : .signIn(mail)
            } catch {
                start = .failed(error.text())
            }
        }
    }
}
