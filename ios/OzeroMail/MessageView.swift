import SwiftUI

/// A conversation, opened at its newest message: its HTML when it has one, otherwise the
/// text. The older messages sit above it, short; a tap shows one. Reply answers the one shown.
struct MessageView: View {
    let mail: Mail
    let message: MessageItem
    let onReply: (MessageItem) -> Void

    @State private var thread: [MessageItem] = []
    @State private var shown: MessageItem?
    /// With the id of its message, so a page is never shown under another message's header.
    @State private var content: Loaded?
    @State private var error: LocalizedStringKey?

    var body: some View {
        let current = shown ?? message
        let loaded = content?.id == current.id ? content?.body : nil
        VStack(alignment: .leading, spacing: 0) {
            header(current).padding([.horizontal, .top], 20)
            let older = thread.filter { $0.id != current.id }
            if !older.isEmpty {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(older, id: \.id) { other in
                            Button {
                                error = nil
                                shown = other
                            } label: {
                                OlderRow(message: other)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .frame(maxHeight: 220)
                .padding(.top, 8)
            }
            Divider().padding(.top, 12)
            if let html = loaded?.html {
                // A new web view per message: it loads its page only once.
                HTMLView(html: html).id(current.id)
            } else {
                ScrollView {
                    Group {
                        if let loaded {
                            Text(loaded.text).textSelection(.enabled)
                        } else if let error {
                            Text(error).foregroundStyle(.secondary)
                        } else {
                            ProgressView().frame(maxWidth: .infinity).padding(32)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(20)
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            HStack {
                Spacer()
                Button("message_reply") { onReply(current) }.buttonStyle(.borderedProminent)
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
            .background(.bar)
        }
        .navigationBarTitleDisplayMode(.inline)
        .task(id: message.id) {
            do {
                if message.threadCount > 1 { thread = try mail.thread(of: message) }
                if message.threadUnread > 0 { try mail.markRead(message) }
            } catch {
                self.error = error.text()
            }
        }
        .task(id: current.id) {
            do {
                content = Loaded(id: current.id, body: try await mail.body(of: current))
            } catch {
                self.error = error.text()
            }
        }
    }

    private func header(_ message: MessageItem) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            message.subjectText
                .font(.title2.weight(.semibold))
            Text(message.sender).fontWeight(.medium)
            if message.sender != message.fromEmail {
                Text(message.fromEmail).font(.footnote).foregroundStyle(.secondary)
            }
            Text(Date(timeIntervalSince1970: TimeInterval(message.date)), format: .dateTime)
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct Loaded {
    let id: Int64
    let body: MessageBody
}

/// Another message of the conversation, on two lines.
private struct OlderRow: View {
    let message: MessageItem

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline) {
                Text(message.sender).fontWeight(.medium).lineLimit(1)
                Spacer()
                Text(shortDate(message.date))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            if !message.snippet.isEmpty {
                Text(message.snippet).font(.footnote).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 6)
        .contentShape(.rect)
    }
}
