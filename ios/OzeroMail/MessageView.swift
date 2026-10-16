import SwiftUI

/// One message: its HTML when it has one, otherwise the text.
struct MessageView: View {
    let mail: Mail
    let message: MessageItem

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
        .navigationBarTitleDisplayMode(.inline)
        .task(id: message.id) {
            if !message.seen { mail.markRead(message) }
            do {
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
