import SwiftUI

/// Adds an IMAP account with its SMTP server. The fields sit in the lower half, in reach of the thumb.
struct SignInView: View {
    let mail: Mail
    let onSignedIn: () -> Void

    @State private var email = ""
    @State private var password = ""
    // The server follows the email until the person types one of their own.
    @State private var server: String?
    @State private var port = String(Mail.imapsPort)
    @State private var smtpServer: String?
    @State private var smtpPort = String(Mail.smtpsPort)
    @State private var busy = false
    @State private var error: LocalizedStringKey?

    private var suggested: String? {
        suggestedImapHost(email: email)
    }

    private var host: String {
        (server ?? suggested ?? "").trimmingCharacters(in: .whitespaces)
    }

    private var smtpHost: String {
        (smtpServer ?? suggestedSmtpHost(email: email) ?? "").trimmingCharacters(in: .whitespaces)
    }

    private var imap: Endpoint? { endpoint(host, port) }
    private var smtp: Endpoint? { endpoint(smtpHost, smtpPort) }

    private var ready: Bool {
        !busy && suggested != nil && !password.isEmpty && imap != nil && smtp != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("signin_title").font(.largeTitle.bold())
            Spacer()
            TextField("signin_email", text: $email)
                .textContentType(.emailAddress)
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            SecureField("signin_password", text: $password)
                .textContentType(.password)
            ServerFields(
                label: "signin_server", host: Binding(get: { host }, set: { server = $0 }), port: $port)
            ServerFields(
                label: "signin_smtp_server", host: Binding(get: { smtpHost }, set: { smtpServer = $0 }),
                port: $smtpPort)
            if let error {
                Text(error).foregroundStyle(.red)
            }
            Button(action: submit) {
                Text("signin_action").frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(!ready)
        }
        .textFieldStyle(.roundedBorder)
        .padding(24)
        .onSubmit(submit)
    }

    private func submit() {
        guard ready, let imap, let smtp else { return }
        busy = true
        error = nil
        Task {
            defer { busy = false }
            do {
                try await mail.signIn(
                    email: email.trimmingCharacters(in: .whitespaces), password: password,
                    imap: imap, smtp: smtp)
                onSignedIn()
            } catch {
                self.error = error.text()
            }
        }
    }
}

private func endpoint(_ host: String, _ port: String) -> Endpoint? {
    guard !host.isEmpty, let number = UInt16(port), number > 0 else { return nil }
    return Endpoint(host: host, port: number)
}

/// A server with its port on one line.
private struct ServerFields: View {
    let label: LocalizedStringKey
    @Binding var host: String
    @Binding var port: String

    var body: some View {
        HStack(spacing: 12) {
            TextField(label, text: $host)
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            TextField("signin_port", text: $port)
                .keyboardType(.numberPad)
                .frame(width: 80)
        }
    }
}
