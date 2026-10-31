import SwiftUI

/// What the settings change; each saves at once.
struct SettingsActions {
    let onAdd: () -> Void
    let onInAll: (AccountItem, Bool) -> Void
    let onNotify: (AccountItem, Bool) -> Void
    let onColor: (AccountItem, Int) -> Void
    let onLabel: (AccountItem, String) -> Void
    let onSender: (AccountItem, String) -> Void
    let onSignature: (String) -> Void
    let onAccountSignature: (AccountItem, String) -> Void
    let onSignIn: (AccountItem) -> Void
    let onKeepDays: (AccountItem, Int) -> Void
    let onNewMail: (AccountItem, NewMail) -> Void
    let onRemove: (AccountItem) -> Void
    let onWrite: () -> Void
}

/// A page of the settings, pushed over the inbox.
enum SettingsPage: Hashable {
    case home
    case notifications
    case swipes
    case signature
    case reading
    case account(Int64)
}

/// The settings' pages: the first one lists the accounts, then the mail's settings, reading and
/// the app, each row with what it is set to.
struct SettingsView: View {
    let page: SettingsPage
    let accounts: [AccountItem]
    /// The default one, for accounts without their own.
    let signature: String
    @Binding var swipes: Swipes
    let mail: Mail
    let actions: SettingsActions

    var body: some View {
        switch page {
        case .home:
            AppSettings(accounts: accounts, swipes: swipes, signature: signature, mail: mail, actions: actions)
        case .notifications:
            NotificationSettings(accounts: accounts, actions: actions)
        case .swipes:
            SwipeSettings(swipes: $swipes)
        case .signature:
            Form {
                SignatureSection(saved: signature, label: "signature_title", placeholder: "", hint: "signature_hint",
                                 onSave: actions.onSignature)
            }
            .navigationTitle("signature_title")
        case .reading:
            ReadingSettings(mail: mail)
        case .account(let id):
            // Gone, say removed meanwhile: nothing to show.
            if let account = accounts.first(where: { $0.id == id }) {
                AccountSettings(account: account, inAll: account.inAllFixed ? nil : account.inAll,
                                signature: signature, actions: actions)
            }
        }
    }
}

private struct AppSettings: View {
    let accounts: [AccountItem]
    let swipes: Swipes
    let signature: String
    let mail: Mail
    let actions: SettingsActions

    // Watched, so the row shows the size picked on the Reading page.
    @AppStorage("text_size") private var textSize = Settings.usualSize
    @State private var imagesAlways: Bool?

    var body: some View {
        List {
            Section("accounts_title") {
                ForEach(accounts, id: \.id) { account in
                    NavigationLink(value: Route.settings(.account(account.id))) {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(verbatim: account.name).lineLimit(1)
                                // The address, or what the account is when its name is the address.
                                let isAddress = account.name.trimmingCharacters(in: .whitespaces).caseInsensitiveCompare(account.email) == .orderedSame
                                let place = isAddress ? account.kind.title : account.email
                                let quiet = account.notify ? "" : " · " + tr("account_quiet")
                                Text(verbatim: place + quiet)
                                    .font(.small).foregroundStyle(.secondary).lineLimit(1)
                            }
                        } icon: {
                            AccountDot(color: account.tint, size: 12)
                        }
                    }
                }
                SheetAction(icon: "plus", text: "inbox_add_account", color: .ozeroAccent, action: actions.onAdd)
            }
            Section("mail_settings") {
                ValueLink(page: .notifications, icon: "bell", title: "notify_title", value: notifySummary(accounts))
                ValueLink(page: .swipes, icon: "hand.draw", title: "swipes_title",
                          value: swipes.summary)
                ValueLink(page: .signature, icon: "signature", title: "signature_title",
                          value: signature.components(separatedBy: .newlines).first { !$0.isBlank } ?? tr("signature_none"))
            }
            Section("settings_reading") {
                ValueLink(page: .reading, icon: "textformat.size", title: "reading_size", value: sizeNames[min(max(textSize, 0), sizeNames.count - 1)])
                ValueLink(page: .reading, icon: "photo", title: "images_title",
                          value: imagesAlways.map { tr($0 ? "images_always" : "images_ask") })
            }
            Section {
                SheetAction(icon: "envelope", text: "feedback_write", color: .ozeroAccent, action: actions.onWrite)
            } footer: {
                Text(verbatim: "\(tr("app_name")) \(appVersion)")
                    .frame(maxWidth: .infinity)
                    .padding(.top, 12)
            }
        }
        .listStyle(.insetGrouped)
        .task { imagesAlways = try? await mail.imagesAlways() }
        .navigationTitle("settings_title")
    }
}

/// A row that opens a page, with what it is set to.
private struct ValueLink: View {
    let page: SettingsPage
    let icon: String
    let title: LocalizedStringKey
    let value: String?

    var body: some View {
        NavigationLink(value: Route.settings(page)) {
            LabeledContent {
                if let value { Text(verbatim: value).lineLimit(1) }
            } label: {
                // Quiet, so the accent marks what acts.
                Label { Text(title) } icon: { Image(systemName: icon).foregroundStyle(.secondary) }
            }
        }
    }
}

/// Which accounts notify, in a few words.
private func notifySummary(_ accounts: [AccountItem]) -> String {
    let on = accounts.filter(\.notify)
    let off = accounts.filter { !$0.notify }
    if off.isEmpty { return tr("notify_all") }
    if on.isEmpty { return tr("notify_off") }
    let names = { (some: [AccountItem]) in some.map(\.name).joined(separator: ", ") }
    return off.count <= on.count ? tr("notify_but", names(off)) : tr("notify_only", names(on))
}

/// A switch for each account, and what a notification is for.
private struct NotificationSettings: View {
    let accounts: [AccountItem]
    let actions: SettingsActions

    var body: some View {
        Form {
            Section {
                ForEach(accounts, id: \.id) { account in
                    Toggle(isOn: Binding(get: { account.notify }, set: { actions.onNotify(account, $0) })) {
                        Label { Text(verbatim: account.name) } icon: { AccountDot(color: account.tint, size: 12) }
                    }
                }
            } footer: {
                Text("notify_hint")
            }
        }
        .navigationTitle("notify_title")
    }
}

extension AccountKind {
    /// What the provider calls itself.
    var title: String {
        switch self {
        case .imap: tr("signin_imap")
        case .gmail: tr("kind_gmail")
        case .graph: tr("kind_outlook")
        case .exchange: tr("signin_exchange")
        }
    }
}

/// What each place in `Settings.textScales` is called.
private let sizeNames = ["reading_size_smaller", "reading_size_usual", "reading_size_larger", "reading_size_large",
                         "reading_size_largest"].map { tr($0) }

/// Which action each swipe does, short and long, tried on a row each time one changes.
private struct SwipeSettings: View {
    @Binding var swipes: Swipes
    @State private var offset: CGFloat = 0
    @State private var shown = SwipeAction.done

    var body: some View {
        Form {
            Section { preview.listRowInsets(EdgeInsets()) }
            side("swipes_left", $swipes.left)
            long("swipes_left_long", $swipes.leftLong)
            side("swipes_right", $swipes.right)
            long("swipes_right_long", $swipes.rightLong)
        }
        .navigationTitle("swipes_title")
        .onChange(of: swipes.left) { play(-1, swipes.left) }
        .onChange(of: swipes.right) { play(1, swipes.right) }
        // A long one turned off shows the short one again.
        .onChange(of: swipes.leftLong) { play(-1, swipes.leftLong ?? swipes.left, far: swipes.leftLong != nil) }
        .onChange(of: swipes.rightLong) { play(1, swipes.rightLong ?? swipes.right, far: swipes.rightLong != nil) }
        .onAppear { play(-1, swipes.left) }
    }

    /// A message row: the side it slides from shows that swipe's action.
    private var preview: some View {
        ZStack(alignment: offset < 0 ? .trailing : .leading) {
            Rectangle().fill(offset == 0 ? Color.clear : shown.tint)
            Image(systemName: shown.icon).foregroundStyle(.white).padding(.horizontal, 24)
            VStack(alignment: .leading, spacing: 2) {
                Text("swipes_sample_from").fontWeight(.medium)
                Text("swipes_sample_subject").font(.text).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .background(Color(.secondarySystemGroupedBackground))
            .offset(x: offset)
        }
        .frame(height: 64)
        .clipped()
    }

    private func play(_ way: CGFloat, _ action: SwipeAction, far: Bool = false) {
        shown = action
        withAnimation(.smooth(duration: 0.35)) { offset = way * (far ? 220 : 96) } completion: {
            withAnimation(.smooth(duration: 0.35)) { offset = 0 }
        }
    }

    private func side(_ title: LocalizedStringKey, _ selection: Binding<SwipeAction>) -> some View {
        Picker(title, selection: selection) {
            ForEach(SwipeAction.allCases) { Label($0.label, systemImage: $0.icon).tag($0) }
        }
        .pickerStyle(.inline)
    }

    /// A long swipe's action, or none.
    private func long(_ title: LocalizedStringKey, _ selection: Binding<SwipeAction?>) -> some View {
        Picker(title, selection: selection) {
            Label("swipes_off", systemImage: "xmark").tag(SwipeAction?.none)
            ForEach(SwipeAction.allCases) { Label($0.label, systemImage: $0.icon).tag(Optional($0)) }
        }
        .pickerStyle(.inline)
    }
}

/// The size of mail's text, tried on a paragraph as it changes, then whether images load without
/// asking, and the senders whose images do.
private struct ReadingSettings: View {
    let mail: Mail

    @State private var size = Double(Settings.textSize)
    @State private var always: Bool?
    @State private var senders: [String] = []

    var body: some View {
        Form {
            Section {
                VStack(alignment: .leading, spacing: 6) {
                    Text("reading_sample_from").font(.smallStrong).foregroundStyle(.secondary)
                    Text("reading_sample")
                        .font(.reading(scale: Settings.textScales[Int(size)]))
                        .lineSpacing(6 * Settings.textScales[Int(size)])
                }
                .padding(.vertical, 6)
                HStack(spacing: 12) {
                    Text(verbatim: "A").font(.system(size: 13, design: .serif)).foregroundStyle(.secondary)
                    Slider(value: $size, in: 0...Double(Settings.textScales.count - 1), step: 1)
                    Text(verbatim: "A").font(.system(size: 21, design: .serif)).foregroundStyle(.secondary)
                }
            } header: {
                Text("reading_size")
            } footer: {
                Text(verbatim: "\(sizeNames[Int(size)]). \(tr("reading_size_hint"))")
            }
            if let always {
                Section {
                    Picker("images_title", selection: Binding(get: { always }, set: setAlways)) {
                        Text("images_ask").tag(false)
                        Text("images_always").tag(true)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                } header: {
                    Text("images_title")
                } footer: {
                    Text(always ? "images_always_hint" : "images_ask_hint")
                }
                if !always && !senders.isEmpty {
                    Section("images_allowed") {
                        ForEach(senders, id: \.self) { email in
                            Text(verbatim: email).lineLimit(1)
                        }
                        .onDelete { places in
                            for email in places.map({ senders[$0] }) { forbid(email) }
                        }
                    }
                }
            }
        }
        .navigationTitle("settings_reading")
        .onChange(of: size) { _, now in Settings.textSize = Int(now) }
        .task {
            always = try? await mail.imagesAlways()
            senders = (try? await mail.imageSenders()) ?? []
        }
    }

    private func setAlways(_ on: Bool) {
        always = on
        Task {
            do { try await mail.setImagesAlways(on) } catch { always = !on }
        }
    }

    private func forbid(_ email: String) {
        senders.removeAll { $0 == email }
        Task {
            do { try await mail.forbidImages(of: email) } catch { senders.append(email) }
        }
    }
}

/// One account: who it is and where it lives, then how others see it, what it looks like on this
/// phone, its new mail and its sign-in, and a way to remove it. One the server signed out offers
/// to sign in again first.
private struct AccountSettings: View {
    let account: AccountItem
    /// Whether the unified inbox takes it in; nil when that cannot change.
    let inAll: Bool?
    /// The default one, shown while the account has none.
    let signature: String
    let actions: SettingsActions

    @State private var confirming = false

    var body: some View {
        Form {
            Section {
                HStack(spacing: 14) {
                    AccountInitial(name: account.name, color: account.tint, size: 40)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: account.email).lineLimit(1)
                        Text(verbatim: [account.kind.title, account.server].filter { !$0.isEmpty }.joined(separator: " · "))
                            .font(.small).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
            if account.needsSignIn {
                Section {
                    SheetAction(icon: "chevron.right", text: "signin_again_title", color: .ozeroError) {
                        actions.onSignIn(account)
                    }
                } footer: {
                    Text("account_signed_out")
                }
            }
            Section {
                NameField(saved: account.senderName,
                          label: "account_sender_name", placeholder: account.email, capitalization: .words) {
                    actions.onSender(account, $0)
                }
            } header: {
                Text("account_seen_as")
            }
            SignatureSection(saved: account.signature, label: "account_signature", placeholder: signature,
                             hint: "account_signature_hint") { actions.onAccountSignature(account, $0) }
            Section {
                NameField(saved: account.label, label: "account_name", placeholder: account.email, capitalization: .sentences) {
                    actions.onLabel(account, $0)
                }
            } header: {
                Text("account_on_phone")
            }
            Section("account_color") {
                Swatches(current: Int(account.colorIndex)) { actions.onColor(account, $0) }
                if let inAll {
                    Toggle("all_in", isOn: Binding(get: { inAll }, set: { actions.onInAll(account, $0) }))
                }
            }
            Section {
                Toggle(isOn: Binding(get: { account.notify }, set: { actions.onNotify(account, $0) })) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("notify_title")
                        Text("account_notify_hint").font(.small).foregroundStyle(.secondary)
                    }
                }
                Picker("account_check", selection: Binding(get: { account.newMail }, set: { actions.onNewMail(account, $0) })) {
                    ForEach(allNewMail, id: \.check) { Text($0.label).tag($0.check) }
                }
                .pickerStyle(.menu)
                Picker("account_keep", selection: Binding(get: { Int(account.keepDays) }, set: { actions.onKeepDays(account, $0) })) {
                    ForEach(keepDays, id: \.days) { Text($0.label).tag($0.days) }
                }
                // Six spans are too many for segments on a phone.
                .pickerStyle(.menu)
            } header: {
                Text("account_new_mail")
            } footer: {
                let check = switch account.newMail {
                case .manually: tr("account_new_mail_manually_hint")
                case .every: tr("account_new_mail_every_hint")
                }
                Text(verbatim: "\(check) \(tr("account_keep_hint"))")
            }
            if !account.needsSignIn {
                Section("account_sign_in") {
                    Button { actions.onSignIn(account) } label: {
                        // Google asks for its own sign-in; the rest for the new password.
                        let google = account.kind == .gmail
                        Label {
                            Text(LocalizedStringKey(google ? "signin_again_title" : "account_password"))
                        } icon: {
                            Image(systemName: "key").foregroundStyle(.secondary)
                        }
                    }
                    .tint(.primary)
                }
            }
            Section {
                Button("account_remove", role: .destructive) { confirming = true }
            }
        }
        .navigationTitle(Text(verbatim: account.name))
        .alert(Text(verbatim: tr("account_remove_title", account.name)), isPresented: $confirming) {
            Button("action_remove", role: .destructive) { actions.onRemove(account) }
            Button("action_cancel", role: .cancel) {}
        } message: {
            Text("account_remove_text")
        }
    }
}

/// A name of one line, `placeholder` while empty; saved on Done, when it loses focus and when the
/// page closes.
private struct NameField: View {
    let label: LocalizedStringKey
    let placeholder: String
    let capitalization: TextInputAutocapitalization
    let onSave: (String) -> Void

    @State private var text: String
    /// The name last sent, so leaving after Done does not send it again.
    @State private var saved: String
    @FocusState private var editing: Bool

    init(saved: String, label: LocalizedStringKey, placeholder: String, capitalization: TextInputAutocapitalization,
         onSave: @escaping (String) -> Void) {
        self.label = label
        self.placeholder = placeholder
        self.capitalization = capitalization
        self.onSave = onSave
        _text = State(initialValue: saved)
        _saved = State(initialValue: saved)
    }

    var body: some View {
        HStack {
            TextField(text: $text, prompt: Text(verbatim: placeholder)) { Text(label) }
                .textInputAutocapitalization(capitalization)
                .submitLabel(.done)
                .focused($editing)
                .onSubmit(save)
            if text.trimmingCharacters(in: .whitespaces) != saved {
                Button("action_save") {
                    editing = false
                    save()
                }
                .fontWeight(.medium)
            }
        }
        .onDisappear(perform: save)
        .onChange(of: editing) { _, now in if !now { save() } }
    }

    private func save() {
        let name = text.trimmingCharacters(in: .whitespaces)
        guard name != saved else { return }
        saved = name
        onSave(text)
    }
}

/// When the phone looks for an account's new mail with the app closed, and what to call it.
private let allNewMail: [(check: NewMail, label: LocalizedStringKey)] = [
    (.every(minutes: 15), "account_new_mail_quarter"), (.every(minutes: 60), "account_new_mail_hourly"),
    (.manually, "account_new_mail_manually"),
]

/// How many days back an account's mail stays on the phone, 0 for all of it, and what to call it.
private let keepDays: [(days: Int, label: LocalizedStringKey)] = [
    (3, "account_keep_three_days"), (7, "account_keep_week"), (30, "account_keep_month"), (90, "account_keep_quarter"), (365, "account_keep_year"), (0, "account_keep_all"),
]

/// A signature of a few lines under `label`, `placeholder` while empty; saved when it loses focus
/// and when the page closes.
private struct SignatureSection: View {
    let label: LocalizedStringKey
    let placeholder: String
    let hint: LocalizedStringKey
    let onSave: (String) -> Void

    @State private var text: String
    /// What was last sent, so leaving after a save does not send it again.
    @State private var sent: String
    @FocusState private var editing: Bool

    init(saved: String, label: LocalizedStringKey, placeholder: String, hint: LocalizedStringKey,
         onSave: @escaping (String) -> Void) {
        self.label = label
        self.placeholder = placeholder
        self.hint = hint
        self.onSave = onSave
        _text = State(initialValue: saved)
        _sent = State(initialValue: saved)
    }

    var body: some View {
        Section {
            TextField(text: $text, prompt: Text(verbatim: placeholder), axis: .vertical) { Text(label) }
                .lineLimit(3...8)
                .focused($editing)
        } header: {
            Text(label)
        } footer: {
            Text(hint)
        }
        .onDisappear(perform: save)
        .onChange(of: editing) { _, now in if !now { save() } }
    }

    private func save() {
        // Trimmed at the end only, as it is kept.
        let trimmed = text.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression)
        guard trimmed != sent else { return }
        sent = trimmed
        onSave(text)
    }
}


/// The account colours in rows; the current one has a ring.
private struct Swatches: View {
    let current: Int
    let onPick: (Int) -> Void

    // In rainbow order, grey last, so each row reads as a run of hues; the saved place stays the same.
    private static let colors = accountColors().enumerated().sorted { hueOrder($0.element) < hueOrder($1.element) }

    var body: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible()), count: 5), spacing: 16) {
            ForEach(Self.colors, id: \.offset) { index, color in
                let on = index == current
                Button { onPick(index) } label: {
                    Circle()
                        .fill(Color(rgb: color))
                        .frame(width: 40, height: 40)
                        .overlay {
                            if on { Image(systemName: "checkmark").font(.footnote.bold()).foregroundStyle(.white) }
                        }
                        .padding(4)
                        .overlay(Circle().stroke(on ? Color.primary : .clear, lineWidth: 2))
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(on ? .isSelected : [])
            }
        }
        .padding(.vertical, 8)
    }
}

/// Where a colour falls from red round to pink; greys go after all of them.
private func hueOrder(_ rgb: UInt32) -> Double {
    var hue: CGFloat = 0
    var saturation: CGFloat = 0
    UIColor(Color(rgb: rgb)).getHue(&hue, saturation: &saturation, brightness: nil, alpha: nil)
    // Reds just under 360° belong at the start.
    return saturation < 0.35 ? 1000 : (hue * 360 + 20).truncatingRemainder(dividingBy: 360)
}
