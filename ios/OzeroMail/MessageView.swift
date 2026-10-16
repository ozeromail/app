import SwiftUI

/// One message: its HTML when it has one, otherwise the text. Reply sits at the bottom.
struct MessageView: View {
    let mail: Mail
    let message: MessageItem
    let onReply: () -> Void

    @State private var content: MessageBody?
    @State private var error: LocalizedStringKey?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header.padding([.horizontal, .top], 20)
            Divider().padding(.top, 12)
            if let html = content?.html {
                HTMLView(html: html)
            } else {
                ScrollView {
                    Group {
                        if let content {
                            Text(content.text).textSelection(.enabled)
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
                Button("message_reply", action: onReply).buttonStyle(.borderedProminent)
            }
            .padding(.horizontal)
            .padding(.vertical, 8)
            .background(.bar)
        }
        .navigationBarTitleDisplayMode(.inline)
        .task(id: message.id) {
            do {
                if !message.seen { try mail.markRead(message) }
                content = try await mail.body(of: message)
            } catch {
                self.error = error.text()
            }
        }
    }

    private var header: some View {
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
