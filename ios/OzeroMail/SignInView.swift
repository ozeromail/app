import AuthenticationServices
import SwiftUI
import UniformTypeIdentifiers

/// A mail service with known servers, a page where its app passwords are made, and `hint` on
/// what making one takes.
private enum Provider {
    // For those Google sign-in does not let in yet; Gmail addresses go to Google sign-in.
    case gmail, icloud, yahoo, yandex, mailru

    /// The service an address is at, if sign-in knows it.
    static func of(_ email: String) -> Provider? {
        switch knownService(email: email) {
        case .iCloud: .icloud
        case .yahoo: .yahoo
        case .yandex: .yandex
        case .mailRu: .mailru
        case .google, nil: nil
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

/// Whether an address signs in with Google.
private func isGoogle(_ email: String) -> Bool { knownService(email: email) == .google }

/// Where the person is, past the first step.
private enum Step: Hashable {
    /// A known service, its address typed on the first step; for Gmail, on this one.
    case known(Provider)
    /// An address sign-in does not know: its servers are looked for.
    case found
    /// Servers typed, IMAP or Exchange; `lost` once none was found for the address.
    case servers(imap: Bool, lost: Bool = false)
    /// The ways in that take more than an address.
    case manual

    /// How far it is from the first step, for which way the screens slide.
    var depth: Int {
        switch self {
        case .found, .manual: 1
        case .known(let provider): provider == .gmail ? 2 : 1
        case .servers: 2
        }
    }

    var lost: Bool {
        if case .servers(_, true) = self { true } else { false }
    }

    /// One identity while lost, so switching IMAP and Exchange does not slide.
    var screen: Step { lost ? .servers(imap: true, lost: true) : self }
}

/// Adds an account, starting from its address: Google signs in in the system's sheet, a known
/// service asks for an app password, and any other address for its password, its servers then
/// found on their own; only when none is found are they typed. The first step is also the
/// app's first screen, with its name over it. Everything to tap sits in the lower half.
/// With `onCancel`, for a further account, the back arrow leaves it.
/// With `again`, it signs in to that saved account again instead, keeping its mail.
struct SignInView: View {
    let mail: Mail
    var again: AccountItem?
    var onCancel: (() -> Void)?
    let onSignedIn: () -> Void

    // Nil on the first step.
    @State private var step: Step?
    @State private var forward = true
    @State private var email = ""
    @State private var password = ""
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
                } else {
                    current
                        .id(step?.screen)
                        .transition(
                            .asymmetric(
                                insertion: .move(edge: forward ? .trailing : .leading),
                                removal: .move(edge: forward ? .leading : .trailing)
                            ).combined(with: .opacity))
                }
            }
            .frame(maxHeight: .infinity)
            if let error {
                Text(error).foregroundStyle(Color.ozeroError).padding(.top, 12)
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 24)
        .middle(Room.list)
        .animation(.smooth(duration: 0.42), value: step?.screen)
        // Not while signing in: leaving would cut it off halfway.
        .interactiveDismissDisabled(busy)
    }

    @ViewBuilder private var current: some View {
        // Keeps the word that no server was found.
        let onKind: ((Bool) -> Void)? = step?.lost == true ? { step = .servers(imap: $0, lost: true) } : nil
        switch step {
        case nil:
            Start(
                email: $email, busy: busy, onCancel: onCancel, onGoogle: google, onNext: next,
                onManual: { go(.manual) })
        case .manual:
            Manual(busy: busy, onBack: back, onPick: { go($0) })
        case .servers(imap: false, let lost):
            // The server was not found: the fields to type it come out.
            Exchange(
                email: $email, password: $password, typed: !lost, onKind: onKind, busy: busy,
                manual: lost || error == "error_no_server" || error == "error_certificate_needed", onBack: back
            ) { server, login, certificate in
                attempt {
                    try await mail.signInExchange(
                        email: address, password: password, server: server, login: login,
                        certificate: certificate)
                }
            }
        case .known(let provider):
            credentials(provider: provider, typed: provider == .gmail, servers: false, onKind: nil)
        case .found:
            credentials(provider: nil, typed: false, servers: false, onKind: nil)
        case .servers(imap: true, let lost):
            credentials(provider: nil, typed: !lost, servers: true, onKind: onKind)
        }
    }

    private func credentials(provider: Provider?, typed: Bool, servers: Bool, onKind: ((Bool) -> Void)?) -> some View {
        Credentials(
            provider: provider, email: $email, password: $password, typed: typed, servers: servers,
            onKind: onKind, busy: busy, onBack: back, onAppPasswords: { openURL($0) }
        ) { imap, smtp in
            attempt {
                if let imap, let smtp {
                    try await mail.signIn(email: address, password: password, imap: imap, smtp: smtp)
                } else {
                    do {
                        try await mail.signInFound(email: address, password: password)
                    } catch CoreError.NotFound {
                        // Typed then, IMAP first: the more common.
                        step = .servers(imap: true, lost: true)
                        throw CoreError.NotFound
                    }
                }
            }
        }
    }

    private var address: String { email.trimmingCharacters(in: .whitespacesAndNewlines) }

    private func go(_ to: Step?) {
        forward = (to?.depth ?? 0) >= (step?.depth ?? 0)
        // A password typed for one way in is not sent to another.
        password = ""
        step = to
        error = nil
    }

    private func back() {
        switch step {
        case .known(.gmail): go(.manual)
        case .servers(_, let lost): go(lost ? nil : .manual)
        default: go(nil)
        }
    }

    private func next() {
        guard !busy, suggestedImapHost(email: email) != nil else { return }
        if isGoogle(email) {
            self.google()
        } else {
            go(Provider.of(email).map { Step.known($0) } ?? .found)
        }
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
                let url = try await mail.startGoogle(again: again, email: isGoogle(address) ? address : nil)
                // The scheme the core asked Google to come back to.
                let back = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?
                    .first { $0.name == "redirect_uri" }?.value.flatMap { URL(string: $0)?.scheme } ?? ""
                let answer = try await webAuthentication.authenticate(
                    using: url, callback: .customScheme(back), additionalHeaderFields: [:])
                // Back to the first step, where Google is, from whatever step the person was on.
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

/// The first step, and the app's first screen: its name, then Google, or an address whose
/// service is told by its domain. For a further account, a back arrow and a title instead.
private struct Start: View {
    @Binding var email: String
    let busy: Bool
    let onCancel: (() -> Void)?
    let onGoogle: () -> Void
    let onNext: () -> Void
    let onManual: () -> Void

    var body: some View {
        Fitting {
            VStack(alignment: .leading, spacing: 12) {
                if let onCancel {
                    StepBack(label: "action_back", onBack: onCancel, busy: busy)
                    Text("signin_title").font(.largeTitle.bold())
                } else {
                    Image("Mark")
                        .resizable()
                        .frame(width: 76, height: 76)
                        .padding(.top, 56)
                        .padding(.bottom, 6)
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("app_name").font(.largeTitle.bold())
                        Text("signin_welcome").font(.title3).foregroundStyle(.secondary)
                    }
                }
                Spacer()
                Button(action: onGoogle) {
                    Text("signin_google").frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .tint(.primary)
                .controlSize(.large)
                .disabled(busy)
                HStack(spacing: 12) {
                    VStack { Divider() }
                    Text("signin_or").font(.footnote).foregroundStyle(.secondary)
                    VStack { Divider() }
                }
                EmailField(text: $email)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(onNext)
                SubmitButton(
                    label: isGoogle(email) ? "signin_google" : "signin_next", busy: busy,
                    ready: !busy && suggestedImapHost(email: email) != nil, action: onNext)
                Button("signin_manual", action: onManual)
                    .frame(maxWidth: .infinity)
                    .disabled(busy)
                if onCancel == nil {
                    Text("signin_trust")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                }
            }
        }
    }
}

/// The ways in that take more than an address, as a list low on the screen.
private struct Manual: View {
    let busy: Bool
    let onBack: () -> Void
    let onPick: (Step) -> Void

    var body: some View {
        Fitting {
            VStack(alignment: .leading, spacing: 12) {
                StepBack(label: "action_back", onBack: onBack, busy: busy)
                Text("signin_manual").font(.largeTitle.bold())
                Spacer()
                VStack(spacing: 0) {
                    Choice(label: "signin_exchange", enabled: !busy) { onPick(.servers(imap: false)) }
                    Divider().padding(.leading, 16)
                    Choice(label: "signin_other", enabled: !busy) { onPick(.servers(imap: true)) }
                    Divider().padding(.leading, 16)
                    Choice(label: "signin_gmail", enabled: !busy) { onPick(.known(.gmail)) }
                }
                .background(Color.ozeroCard, in: .rect(cornerRadius: 16))
            }
        }
    }
}

/// A row of a list of ways in.
private struct Choice: View {
    let label: LocalizedStringKey
    let enabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Text(label).font(.body.weight(.medium)).foregroundStyle(.primary)
                Spacer()
                Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 16)
            .frame(minHeight: 56)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

/// IMAP or Exchange, for servers typed once none was found.
private struct Kinds: View {
    let imap: Bool
    let enabled: Bool
    let onPick: (Bool) -> Void

    var body: some View {
        Picker(selection: Binding(get: { imap }, set: onPick)) {
            Text("signin_imap").tag(true)
            Text("signin_exchange").tag(false)
        } label: {
            EmptyView()
        }
        .pickerStyle(.segmented)
        .disabled(!enabled)
    }
}

/// The address field.
private struct EmailField: View {
    @Binding var text: String

    var body: some View {
        TextField("signin_email", text: $text)
            .textContentType(.emailAddress)
            .keyboardType(.emailAddress)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
    }
}

/// A step's title: what was picked when the address is `typed` there, else the address.
private struct StepTitle: View {
    let typed: Bool
    let label: LocalizedStringKey
    let email: String

    var body: some View {
        if typed {
            Text(label).font(.largeTitle.bold())
        } else {
            Text(verbatim: email.trimmingCharacters(in: .whitespacesAndNewlines)).font(.title2.bold())
        }
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

/// A password for an address. A known service asks for an app password and links to where it is
/// made; with `servers`, the IMAP and SMTP fields are typed too; with neither, the servers are
/// found from the address and `onSubmit` gets none. `typed` asks for the address here, and
/// `onKind` switches to Exchange once no server was found.
private struct Credentials: View {
    let provider: Provider?
    @Binding var email: String
    @Binding var password: String
    let typed: Bool
    let servers: Bool
    let onKind: ((Bool) -> Void)?
    let busy: Bool
    let onBack: () -> Void
    let onAppPasswords: (URL) -> Void
    let onSubmit: (Endpoint?, Endpoint?) -> Void

    // The server follows the email until the person types one of their own.
    @State private var server: String?
    @State private var port = String(Mail.imapsPort)
    @State private var smtpServer: String?
    @State private var smtpPort = String(Mail.smtpsPort)

    // An address with a domain, which is also what the server is guessed from.
    private var suggested: String? { suggestedImapHost(email: email) }
    private var host: String { (server ?? suggested ?? "").trimmingCharacters(in: .whitespaces) }
    private var smtpHost: String {
        (smtpServer ?? suggestedSmtpHost(email: email) ?? "").trimmingCharacters(in: .whitespaces)
    }

    private var imap: Endpoint? {
        provider.map { Endpoint(host: $0.imap, port: Mail.imapsPort) } ?? (servers ? endpoint(host, port) : nil)
    }

    private var smtp: Endpoint? { provider?.smtp ?? (servers ? endpoint(smtpHost, smtpPort) : nil) }

    private var ready: Bool {
        !busy && suggested != nil && !password.isEmpty && (!servers || (imap != nil && smtp != nil))
    }

    var body: some View {
        Fitting {
            VStack(alignment: .leading, spacing: 12) {
                StepBack(label: "action_back", onBack: onBack, busy: busy)
                StepTitle(typed: typed, label: provider == nil ? "signin_other" : "signin_gmail", email: email)
                if let provider {
                    Text(provider.hint).foregroundStyle(.secondary)
                    Button("signin_app_password_make") { onAppPasswords(provider.appPasswords) }
                        .disabled(busy)
                } else if !servers {
                    Text("signin_found_text").foregroundStyle(.secondary)
                }
                Spacer()
                if let onKind {
                    Kinds(imap: true, enabled: !busy, onPick: onKind)
                }
                if typed {
                    EmailField(text: $email)
                }
                SecureField(provider == nil ? "signin_password" : "signin_app_password", text: $password)
                    .textContentType(.password)
                if servers {
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
        guard ready else { return }
        onSubmit(imap, smtp)
    }
}

/// Exchange: a password, the server found from the address. The server and a login other than
/// the address can be typed too, and come out on their own when no server is found; so can a
/// certificate from a .p12 file, for a server that asks for one. `typed` asks for the address
/// here, and `onKind` switches to IMAP once no server was found.
private struct Exchange: View {
    @Binding var email: String
    @Binding var password: String
    let typed: Bool
    let onKind: ((Bool) -> Void)?
    let busy: Bool
    let manual: Bool
    let onBack: () -> Void
    let onSubmit: (String?, String?, String?) -> Void

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
                StepBack(label: "action_back", onBack: onBack, busy: busy)
                StepTitle(typed: typed, label: "signin_exchange", email: email)
                if typed {
                    Text("signin_exchange_text").foregroundStyle(.secondary)
                }
                if !shown {
                    Button("signin_exchange_manual") { typing = true }
                        .disabled(busy)
                }
                Spacer()
                if let onKind {
                    Kinds(imap: false, enabled: !busy, onPick: onKind)
                }
                if typed {
                    EmailField(text: $email)
                }
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
        func entered(_ text: String) -> String? {
            let text = text.trimmingCharacters(in: .whitespaces)
            return shown && !text.isEmpty ? text : nil
        }
        onSubmit(entered(server), entered(login), shown ? certificate : nil)
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
