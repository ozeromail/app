import SwiftUI

/// Adds an IMAP account. The fields sit in the lower half, in reach of the thumb.
struct SignInView: View {
    let mail: Mail
    let onSignedIn: () -> Void

    @State private var email = ""
    @State private var password = ""
    // The server follows the email until the person types one of their own.
    @State private var server: String?
    @State private var port = String(Mail.imapsPort)
    @State private var busy = false
    @State private var error: LocalizedStringKey?

    private var suggested: String? {
        suggestedImapHost(email: email)
    }

    private var host: String {
        (server ?? suggested ?? "").trimmingCharacters(in: .whitespaces)
    }

    private var portNumber: UInt16? {
        UInt16(port).flatMap { $0 > 0 ? $0 : nil }
    }

    private var ready: Bool {
        !busy && suggested != nil && !password.isEmpty && !host.isEmpty && portNumber != nil
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
            TextField("signin_server", text: Binding(get: { host }, set: { server = $0 }))
                .keyboardType(.URL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            TextField("signin_port", text: $port)
                .keyboardType(.numberPad)
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
        guard ready, let portNumber else { return }
        busy = true
        error = nil
        Task {
            defer { busy = false }
            do {
                try await mail.signIn(
                    email: email.trimmingCharacters(in: .whitespaces), password: password,
                    host: host, port: portNumber)
                onSignedIn()
            } catch {
                self.error = error.text()
            }
        }
    }
}
