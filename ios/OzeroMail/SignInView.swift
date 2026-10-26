import AuthenticationServices
import SwiftUI
import UniformTypeIdentifiers

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

/// Where the person is, once past the list: a known service, Exchange, or any IMAP server.
private enum Step: Hashable {
    case provider(Provider)
    case exchange
    case other
}

/// Adds an account in two steps: first where the mail lives, then the sign-in for it. Google
/// goes straight to the browser; known services need only an email and an app password;
/// Exchange finds its server from the email; any other server shows its IMAP and SMTP fields.
/// Everything to tap sits in the lower half.
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
    // What the server or the connection said, for the person's IT team.
    @State private var detail: String?
    @Environment(\.webAuthenticationSession) private var webAuthentication
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Group {
                if let again {
                    Again(account: again, busy: busy, onCancel: onCancel, onGoogle: google) { password in
                        attempt { try await mail.signInAgain(again, password: password) }
                    }
                } else if step == .exchange {
                    // The server was not found: the fields to type it come out.
                    Exchange(busy: busy, manual: error == "error_no_server" || error == "error_certificate_needed", onBack: { go(nil) }) {
                        email, password, server, login, certificate in
                        attempt {
                            try await mail.signInExchange(
                                email: email, password: password, server: server, login: login,
                                certificate: certificate)
                        }
                    }
                    .transition(.asymmetric(insertion: .move(edge: .trailing), removal: .move(edge: .leading)).combined(with: .opacity))
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
                VStack(alignment: .leading, spacing: 4) {
                    Text(error).foregroundStyle(Color.ozeroError)
                    if let detail {
                        Text(verbatim: detail).font(.footnote).foregroundStyle(.secondary)
                    }
                }
                .padding(.top, 12)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 24)
        .middle(Room.list)
        .animation(.smooth(duration: 0.42), value: step)
        // Not while signing in: leaving would cut it off halfway.
        .interactiveDismissDisabled(busy)
    }

    private func go(_ to: Step?) {
        step = to
        error = nil
        detail = nil
    }

    /// Runs a sign-in, busy meanwhile; on success leaves the screen.
    private func attempt(_ signIn: @escaping () async throws -> Void) {
        // Set before the task starts, so a second tap does nothing.
        guard !busy else { return }
        busy = true
        error = nil
        detail = nil
        Task {
            defer { busy = false }
            do {
                try await signIn()
                onSignedIn()
            } catch {
                self.error = error.text()
                detail = error.detail
            }
        }
    }

    /// Google in the system's sign-in sheet; its answer comes back to finish here.
    private func google() {
        guard !busy else { return }
        error = nil
        detail = nil
        busy = true
        Task {
            defer { busy = false }
            do {
                let url = try await mail.startGoogle(again: again)
                // The scheme the core asked Google to come back to.
                let back = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?
                    .first { $0.name == "redirect_uri" }?.value.flatMap { URL(string: $0)?.scheme } ?? ""
                let answer = try await webAuthentication.authenticate(
                    using: url, callback: .customScheme(back), additionalHeaderFields: [:])
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
                Divider().padding(.leading, 16)
                Choice(label: "signin_exchange", enabled: !busy) { onPick(.exchange) }
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
        Fitting {
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
        Fitting {
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
        }
        .textFieldStyle(.roundedBorder)
        .onSubmit(submit)
    }

    private func submit() {
        guard ready, let imap, let smtp else { return }
        onSubmit(email.trimmingCharacters(in: .whitespaces), password, imap, smtp)
    }
}

/// Exchange: an email and a password, the server found from the email. The server and a login
/// other than the email can be typed too, and come out on their own when no server is found;
/// so can a certificate from a .p12 file, for a server that asks for one.
private struct Exchange: View {
    let busy: Bool
    let manual: Bool
    let onBack: () -> Void
    let onSubmit: (String, String, String?, String?, String?) -> Void

    @State private var email = ""
    @State private var password = ""
    @State private var server = ""
    @State private var login = ""
    @State private var certificate: String?
    @State private var picking = false
    /// A .p12 file picked, until its password opens it.
    @State private var p12: Data?
    @State private var p12Password = ""
    @State private var unreadable = false
    @State private var typing = false
    /// Once no server was found, it has to be typed, also after the error is gone.
    @State private var needed = false

    private var shown: Bool { typing || manual || needed }
    private var ready: Bool {
        !busy && suggestedImapHost(email: email) != nil && !password.isEmpty
            && (!(manual || needed) || !server.trimmingCharacters(in: .whitespaces).isEmpty)
    }

    var body: some View {
        Fitting {
            VStack(alignment: .leading, spacing: 12) {
                StepBack(label: "signin_title", onBack: onBack, busy: busy)
                Text("signin_exchange").font(.largeTitle.bold())
                Text("signin_exchange_text").foregroundStyle(.secondary)
                if !shown {
                    Button("signin_exchange_manual") { typing = true }
                        .disabled(busy)
                }
                Spacer()
                TextField("signin_email", text: $email)
                    .textContentType(.emailAddress)
                    .keyboardType(.emailAddress)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                SecureField("signin_password", text: $password)
                    .textContentType(.password)
                if shown {
                    Group {
                        TextField("signin_exchange_server", text: $server)
                            .keyboardType(.URL)
                        // Not a username for autofill: it would fill or clear it.
                        TextField("signin_exchange_login", text: $login)
                        Button(certificate.map { tr("signin_exchange_certificate", $0) }
                            ?? tr("signin_exchange_certificate_choose")) { picking = true }
                            .disabled(busy)
                        if unreadable {
                            Text("error_certificate_unreadable").foregroundStyle(.red)
                        }
                    }
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .transition(.opacity.combined(with: .move(edge: .bottom)))
                }
                SubmitButton(label: "signin_action", busy: busy, ready: ready, action: submit)
            }
        }
        .textFieldStyle(.roundedBorder)
        .onSubmit(submit)
        .animation(.smooth(duration: 0.3), value: shown)
        .onChange(of: manual) { _, manual in
            if manual { needed = true }
        }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.pkcs12]) { result in
            guard let url = try? result.get() else { return }
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            p12 = try? Data(contentsOf: url)
            unreadable = p12 == nil
        }
        .alert(
            "signin_exchange_certificate_password",
            isPresented: Binding(get: { p12 != nil }, set: { if !$0 { p12 = nil } }),
            presenting: p12
        ) { file in
            SecureField("signin_password", text: $p12Password)
            Button("action_done") { addCertificate(file) }
            Button("action_cancel", role: .cancel) { p12Password = "" }
        }
    }

    private func addCertificate(_ file: Data) {
        let password = p12Password
        p12Password = ""
        Task {
            // Opening a .p12 derives its key slowly: off the main thread.
            let name = await Task.detached { try? KeychainCertificates.add(p12: file, password: password) }.value
            if let name { certificate = name }
            unreadable = name == nil
        }
    }

    private func submit() {
        guard ready else { return }
        func typed(_ text: String) -> String? {
            let text = text.trimmingCharacters(in: .whitespaces)
            return shown && !text.isEmpty ? text : nil
        }
        onSubmit(
            email.trimmingCharacters(in: .whitespaces), password, typed(server), typed(login),
            shown ? certificate : nil)
    }
}

/// A step's form: it fills the screen, scrolls when the keyboard leaves too little room, and
/// a drag or a tap outside the fields puts the keyboard away.
private struct Fitting<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        GeometryReader { space in
            ScrollView {
                content.frame(minHeight: space.size.height, alignment: .top)
            }
            .scrollDismissesKeyboard(.interactively)
            .onTapGesture {
                UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
            }
        }
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
