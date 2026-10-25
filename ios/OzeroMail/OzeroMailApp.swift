import SwiftUI

@main
struct OzeroMailApp: App {
    init() {
        Crashes.shared.install()
    }

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
    /// The report of the crash that closed the app last time, until the person decides.
    @State private var crash: String?
    @State private var composingCrash: Composing?

    var body: some View {
        Group {
            switch start {
            case .loading:
                Color.clear
            case .signIn(let mail):
                SignInView(mail: mail) { start = .inbox(mail) }
            case .inbox(let mail):
                InboxView(mail: mail) { start = .signIn(mail) }
            case .failed(let text):
                Text(text).padding(24)
            }
        }
        .task {
            do {
                let mail = try await Mail.shared()
                start = try await mail.hasAccounts() ? .inbox(mail) : .signIn(mail)
                crash = Crashes.last()
            } catch {
                start = .failed(error.text())
            }
        }
        // Dismissed by a stray tap, it asks again next time.
        .alert("crash_title", isPresented: Binding(get: { crash != nil && isInbox }, set: { if !$0 { crash = nil } })) {
            Button("compose_new") {
                if let report = crash { composingCrash = Composing(draft: authorDraft(crash: report)) }
                decided()
            }
            Button("action_dont_send", role: .cancel, action: decided)
        } message: {
            Text("crash_text_ios")
        }
        .fullScreenCover(item: $composingCrash) { composing in
            if case .inbox(let mail) = start {
                ComposeView(
                    mail: mail, composing: composing, onClose: { composingCrash = nil },
                    onSent: { _, _ in composingCrash = nil }, onKept: { composingCrash = nil },
                    onDiscard: { _ in composingCrash = nil })
            }
        }
    }

    private var isInbox: Bool {
        if case .inbox = start { true } else { false }
    }

    private func decided() {
        crash = nil
        Crashes.forget()
    }
}
