import SwiftUI

/// A message being written, and the one it answers.
struct Composing: Identifiable, Equatable {
    let id = UUID()
    var draft: DraftItem
    var original: MessageItem?
}

/// A new message or a reply, on the whole screen. The fields sit low, near the keyboard;
/// the message being answered stays on top. `onSent` gets the outbox id for undo.
struct ComposeView: View {
    let mail: Mail
    let composing: Composing
    let onSent: (Int64, Composing) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var to = ""
    @State private var subject = ""
    @State private var text = ""
    @State private var accountId: Int64 = 0
    @State private var accounts: [AccountItem] = []
    @State private var busy = false
    @State private var error: LocalizedStringKey?

    /// A new message starts from the first account.
    private var from: AccountItem? {
        accounts.first { $0.id == accountId } ?? accounts.first
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let original = composing.original {
                Original(message: original)
            }
            Spacer(minLength: 0)
            TextField("compose_to", text: $to)
                .keyboardType(.emailAddress)
                .textContentType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Divider()
            TextField("compose_subject", text: $subject)
                .font(.title2.bold())
            TextField("compose_text", text: $text, axis: .vertical)
                .lineLimit(1...20)
            if let error {
                Text(error).foregroundStyle(.red)
            }
            HStack {
                Button("action_cancel") { dismiss() }
                Spacer()
                if let from {
                    Menu(from.email) {
                        ForEach(accounts, id: \.id) { account in
                            Button(account.email) { accountId = account.id }
                        }
                    }
                    .lineLimit(1)
                    .disabled(accounts.count < 2)
                }
                Spacer()
                Button("compose_send", action: send)
                    .buttonStyle(.borderedProminent)
                    .disabled(busy || from == nil)
            }
        }
        .padding(20)
        .onAppear {
            to = composing.draft.to
            subject = composing.draft.subject
            text = composing.draft.text
            accountId = composing.draft.accountId
        }
        .task {
            do {
                accounts = try await mail.accounts()
            } catch {
                self.error = error.text()
            }
        }
    }

    private func send() {
        guard !busy, let from else { return }
        busy = true
        error = nil
        let draft = DraftItem(
            accountId: from.id, to: to, subject: subject, text: text,
            replyTo: composing.draft.replyTo)
        Task {
            defer { busy = false }
            do {
                let id = try await mail.send(draft)
                var sent = composing
                sent.draft = draft
                onSent(id, sent)
            } catch {
                self.error = error.text()
            }
        }
    }
}

/// The message being answered, kept short.
private struct Original: View {
    let message: MessageItem

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(message.sender).fontWeight(.medium).lineLimit(1)
            message.subjectText.foregroundStyle(.secondary).lineLimit(1)
            if !message.snippet.isEmpty {
                Text(message.snippet).font(.footnote).foregroundStyle(.secondary).lineLimit(3)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(.quaternary, in: .rect(cornerRadius: 12))
    }
}

extension Composing {
    /// A blank message; account 0 is none, so compose starts from the first account.
    static var new: Composing {
        Composing(draft: DraftItem(accountId: 0, to: "", subject: "", text: "", replyTo: nil))
    }

    /// An answer to `message`, shown above it.
    static func reply(to message: MessageItem) -> Composing {
        Composing(draft: replyDraft(message: message), original: message)
    }
}
