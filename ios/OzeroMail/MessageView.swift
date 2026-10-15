import SwiftUI

/// One message as plain text.
struct MessageView: View {
    let mail: Mail
    let message: MessageItem

    @State private var text: String?
    @State private var error: LocalizedStringKey?

    var body: some View {
        ScrollView {
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
                Divider().padding(.vertical, 12)
                if let text {
                    Text(text)
                } else if let error {
                    Text(error).foregroundStyle(.secondary)
                } else {
                    ProgressView().frame(maxWidth: .infinity).padding(32)
                }
            }
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(20)
        }
        .navigationBarTitleDisplayMode(.inline)
        .task(id: message.id) {
            do {
                text = try await mail.text(of: message)
            } catch {
                self.error = error.text()
            }
        }
    }
}
