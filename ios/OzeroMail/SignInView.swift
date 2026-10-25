import AuthenticationServices
import SwiftUI

/// A mail service with known servers, a page where its app passwords are made, and `hint` on
/// what making one takes.
private enum Provider: String, CaseIterable, Identifiable {
    // For those Google sign-in does not let in yet.
    case gmail, icloud, yahoo, yandex, mailru

    var id: Self { self }

    var label: LocalizedStringKey {
        switch self {
        case .gmail: "signin_gmail"
        case .icloud: "signin_icloud"
        case .yahoo: "signin_yahoo"
        case .yandex: "signin_yandex"
        case .mailru: "signin_mailru"
        }
    }

    var imap: String {
        switch self {
        case .gmail: "imap.gmail.com"
        case .icloud: "imap.mail.me.com"
        case .yahoo: "imap.mail.yahoo.com"
        case .yandex: "imap.yandex.com"
        case .mailru: "imap.mail.ru"
        }
    }

    var smtp: Endpoint {
        switch self {
        case .gmail: Endpoint(host: "smtp.gmail.com", port: Mail.smtpsPort)
        case .icloud: Endpoint(host: "smtp.mail.me.com", port: 587)
        case .yahoo: Endpoint(host: "smtp.mail.yahoo.com", port: Mail.smtpsPort)
        case .yandex: Endpoint(host: "smtp.yandex.com", port: Mail.smtpsPort)
        case .mailru: Endpoint(host: "smtp.mail.ru", port: Mail.smtpsPort)
        }
    }

    var appPasswords: URL {
        switch self {
        case .gmail: URL(string: "https://myaccount.google.com/apppasswords")!
        case .icloud: URL(string: "https://account.apple.com/account/manage")!
        case .yahoo: URL(string: "https://login.yahoo.com/account/security")!
        case .yandex: URL(string: "https://id.yandex.ru/security/app-passwords")!
        case .mailru: URL(string: "https://account.mail.ru/user/2-step-auth/passwords/")!
        }
    }

    var hint: LocalizedStringKey {
        self == .gmail ? "signin_gmail_text" : "signin_app_password_text"
    }
}

/// Where the person is, once past the list: a known service, or any IMAP server.
private enum Step: Hashable {
    case provider(Provider)
    case other
}

/// Adds an account in two steps: first where the mail lives, then the sign-in for it. Google
/// goes straight to the browser; known services need only an email and an app password; any
/// other server shows its IMAP and SMTP fields. Everything to tap sits in the lower half.
/// With `onCancel`, for a further account, the back arrow leaves it.
/// With `again`, it signs in to that saved account again instead, keeping its mail.
struct SignInView: View {
    let mail: Mail
    var again: AccountItem?
    var onCancel: (() -> Void)?
    let onSignedIn: () -> Void

    // Nil while picking.
    @State private var step: Step?
    @State private var busy = false
    @State private var error: LocalizedStringKey?
    @Environment(\.webAuthenticationSession) private var webAuthentication
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Group {
                if let again {
                    Again(account: again, busy: busy, onCancel: onCancel, onGoogle: google) { password in
                        attempt { try await mail.signInAgain(again, password: password) }
                    }
                } else if let step {
                    Credentials(step: step, busy: busy, onBack: { go(nil) }, onAppPasswords: { openURL($0) }) {
                        email, password, imap, smtp in
                        attempt { try await mail.signIn(email: email, password: password, imap: imap, smtp: smtp) }
                    }
                    .id(step)
                    .transition(.asymmetric(insertion: .move(edge: .trailing), removal: .move(edge: .leading)).combined(with: .opacity))
                } else {
                    Choose(busy: busy, onCancel: onCancel, onGoogle: google, onPick: { go($0) })
                        .transition(.asymmetric(insertion: .move(edge: .leading), removal: .move(edge: .trailing)).combined(with: .opacity))
                }
            }
            .frame(maxHeight: .infinity)
            if let error {
                Text(error).foregroundStyle(Color.ozeroError).padding(.top, 12)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 24)
        .animation(.smooth(duration: 0.42), value: step)
        // Not while signing in: leaving would cut it off halfway.
        .interactiveDismissDisabled(busy)
    }

    private func go(_ to: Step?) {
        step = to
        error = nil
    }

    /// Runs a sign-in, busy meanwhile; on success leaves the screen.
    private func attempt(_ signIn: @escaping () async throws -> Void) {
        // Set before the task starts, so a second tap does nothing.
        guard !busy else { return }
        busy = true
        error = nil
        Task {
            defer { busy = false }
            do {
                try await signIn()
                onSignedIn()
            } catch {
                self.error = error.text()
            }
        }
    }

    /// Google in the system's sign-in sheet; its answer comes back to finish here.
    private func google() {
        guard !busy else { return }
        error = nil
        busy = true
        Task {
            defer { busy = false }
            do {
                let url = try await mail.startGoogle(again: again)
                let answer = try await webAuthentication.authenticate(
                    using: url, callback: .customScheme("com.ozeromail.app"), additionalHeaderFields: [:])
                // Back to the list, where Google is, from whatever step the person was on.
                go(nil)
                try await mail.finishGoogle(redirect: answer, again: again)
                onSignedIn()
            } catch ASWebAuthenticationSessionError.canceledLogin {
                // The person closed the sheet: nothing to say.
            } catch {
                self.error = error.text()
            }
        }
    }
}

/// The first step: where the mail lives, as a list low on the screen.
private struct Choose: View {
    let busy: Bool
    let onCancel: (() -> Void)?
    let onGoogle: () -> Void
    let onPick: (Step) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // The arrow's row is there even without it, so the title sits where the next step's does.
            StepBack(label: "action_back", onBack: onCancel, busy: busy)
                .padding(.bottom, 12)
            Text("signin_title").font(.largeTitle.bold())
            Text("signin_text")
                .foregroundStyle(.secondary)
                .padding(.top, 6)
            Spacer()
            VStack(spacing: 0) {
                Choice(label: "signin_google", enabled: !busy, waiting: busy, action: onGoogle)
                // Any other server last.
                ForEach(Provider.allCases) { provider in
                    Divider().padding(.leading, 16)
                    Choice(label: provider.label, enabled: !busy) { onPick(.provider(provider)) }
                }
                Divider().padding(.leading, 16)
                Choice(label: "signin_other", enabled: !busy) { onPick(.other) }
            }
            .background(Color.ozeroCard, in: .rect(cornerRadius: 16))
        }
    }
}

/// A row of the list of services, with a spinner while `waiting` on it.
private struct Choice: View {
    let label: LocalizedStringKey
    let enabled: Bool
    var waiting = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Text(label).font(.body.weight(.medium)).foregroundStyle(.primary)
                Spacer()
                if waiting {
                    ProgressView()
                } else {
                    Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 16)
            .frame(minHeight: 56)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

/// Signing in to a saved account again: Google goes back to the browser, any other server asks
/// only for the new password, as its servers stay the same.
private struct Again: View {
    let account: AccountItem
    let busy: Bool
    let onCancel: (() -> Void)?
    let onGoogle: () -> Void
    let onSubmit: (String) -> Void

    @State private var password = ""

    private var google: Bool { account.kind == .gmail }
    private var ready: Bool { !busy && (google || !password.isEmpty) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            StepBack(label: "action_back", onBack: onCancel, busy: busy)
            Text("signin_again_title").font(.largeTitle.bold())
            Text(tr(google ? "signin_again_google" : "signin_again_password", account.email))
                .foregroundStyle(.secondary)
            Spacer()
            if !google {
                SecureField("signin_password", text: $password)
                    .textContentType(.password)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(submit)
            }
            SubmitButton(label: google ? "signin_again_with_google" : "signin_action", busy: busy, ready: ready, action: submit)
        }
    }

    private func submit() {
        guard ready else { return }
        if google { onGoogle() } else { onSubmit(password) }
    }
}

/// The second step: an email and a password. A known service asks for an app password and links
/// to where it is made; any other server shows its IMAP and SMTP fields too.
private struct Credentials: View {
    let step: Step
    let busy: Bool
    let onBack: () -> Void
    let onAppPasswords: (URL) -> Void
    let onSubmit: (String, String, Endpoint, Endpoint) -> Void

    @State private var email = ""
    @State private var password = ""
    // The server follows the email until the person types one of their own.
    @State private var server: String?
    @State private var port = String(Mail.imapsPort)
    @State private var smtpServer: String?
    @State private var smtpPort = String(Mail.smtpsPort)

    private var provider: Provider? {
        if case .provider(let provider) = step { provider } else { nil }
    }

    // An address with a domain, which is also what the server is guessed from.
    private var suggested: String? { suggestedImapHost(email: email) }
    private var host: String { (server ?? suggested ?? "").trimmingCharacters(in: .whitespaces) }
    private var smtpHost: String {
        (smtpServer ?? suggestedSmtpHost(email: email) ?? "").trimmingCharacters(in: .whitespaces)
    }

    private var imap: Endpoint? {
        provider.map { Endpoint(host: $0.imap, port: Mail.imapsPort) } ?? endpoint(host, port)
    }

    private var smtp: Endpoint? { provider?.smtp ?? endpoint(smtpHost, smtpPort) }

    private var ready: Bool {
        !busy && suggested != nil && !password.isEmpty && imap != nil && smtp != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            StepBack(label: "signin_title", onBack: onBack, busy: busy)
            Text(provider?.label ?? "signin_other").font(.largeTitle.bold())
            if let provider {
                Text(provider.hint).foregroundStyle(.secondary)
                Button("signin_app_password_make") { onAppPasswords(provider.appPasswords) }
                    .disabled(busy)
            }
            Spacer()
            TextField("signin_email", text: $email)
                .textContentType(.emailAddress)
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            SecureField(provider == nil ? "signin_password" : "signin_app_password", text: $password)
                .textContentType(.password)
            if provider == nil {
                ServerFields(label: "signin_server", host: Binding(get: { host }, set: { server = $0 }), port: $port)
                ServerFields(
                    label: "signin_smtp_server", host: Binding(get: { smtpHost }, set: { smtpServer = $0 }),
                    port: $smtpPort)
            }
            SubmitButton(label: "signin_action", busy: busy, ready: ready, action: submit)
        }
        .textFieldStyle(.roundedBorder)
        .onSubmit(submit)
    }

    private func submit() {
        guard ready, let imap, let smtp else { return }
        onSubmit(email.trimmingCharacters(in: .whitespaces), password, imap, smtp)
    }
}

private func endpoint(_ host: String, _ port: String) -> Endpoint? {
    guard !host.isEmpty, let number = UInt16(port), number > 0 else { return nil }
    return Endpoint(host: host, port: number)
}

/// The wide button at the end of a step, with a spinner while busy.
private struct SubmitButton: View {
    let label: LocalizedStringKey
    let busy: Bool
    let ready: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if busy { ProgressView().tint(.white) } else { Text(label) }
            }
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .disabled(!ready)
    }
}

/// A back arrow over a step's title, or the room for one, so every title sits at one height.
private struct StepBack: View {
    let label: LocalizedStringKey
    let onBack: (() -> Void)?
    let busy: Bool

    var body: some View {
        Group {
            if let onBack {
                Button(action: onBack) {
                    Image(systemName: "chevron.left").font(.title3.weight(.semibold))
                        .frame(width: 44, height: 44, alignment: .leading)
                }
                .accessibilityLabel(Text(label))
                .disabled(busy)
            } else {
                Color.clear
            }
        }
        .frame(height: 44)
    }
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
