import SwiftUI

/// What a tap on someone's name shows: their name as in the contacts, every address they have, and
/// what to do with them: write, see all mail with them, or open or add them in the contacts.
struct PersonSheet: View {
    let mail: Mail
    let person: PersonItem
    let onWrite: (String) -> Void
    let onAllMail: (PersonCard) -> Void

    @State private var card: PersonCard?
    @State private var contact: ContactCard.Shown?

    var body: some View {
        let card = card ?? PersonCard(name: person.name, emails: [person.email], contact: nil)
        VStack(spacing: 20) {
            VStack(spacing: 4) {
                Text(verbatim: card.name)
                    .font(.subjectTitle)
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                Text(LocalizedStringKey(card.contact == nil ? "person_not_in_contacts" : "person_in_contacts"))
                    .font(.text)
                    .foregroundStyle(.secondary)
            }
            HStack(spacing: 10) {
                PersonAction(icon: "square.and.pencil", label: "message_write_to") { onWrite(person.email) }
                PersonAction(icon: "tray.full", label: "person_all_mail") { onAllMail(card) }
                if let key = card.contact {
                    PersonAction(icon: "person.crop.circle", label: "person_open_contact") { contact = .contact(key: key) }
                } else {
                    PersonAction(icon: "person.crop.circle.badge.plus", label: "person_add_contact") {
                        contact = .new(name: card.name, email: person.email)
                    }
                }
            }
            VStack(alignment: .leading, spacing: 6) {
                Text("person_addresses")
                    .font(.smallStrong)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 4)
                VStack(spacing: 0) {
                    ForEach(Array(card.emails.enumerated()), id: \.element) { at, email in
                        if at > 0 { Divider().padding(.leading, 16) }
                        // A tap writes to the address; holding it offers to copy it.
                        Button { onWrite(email) } label: {
                            Text(verbatim: email)
                                .lineLimit(1)
                                .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
                                .padding(.horizontal, 16)
                                .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .contextMenu {
                            Button("message_copy_address", systemImage: "doc.on.doc") { UIPasteboard.general.string = email }
                        }
                    }
                }
                .background(Color.ozeroCard, in: .rect(cornerRadius: 16))
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .padding(.top, 28)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .task { await load() }
        .sheet(item: $contact) { shown in
            ContactCard(shown: shown) {
                contact = nil
                // The person may be in the contacts now, or named otherwise.
                Task {
                    await mail.giveContacts()
                    await load()
                }
            }
            .ignoresSafeArea()
        }
    }

    private func load() async {
        let name = person.name == person.email ? nil : person.name
        if let found = try? await mail.person(email: person.email, name: name) { card = found }
    }
}

/// One of the person's actions: an icon over its name, in a tile, of glass on iOS 26.
private struct PersonAction: View {
    let icon: String
    let label: LocalizedStringKey
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                Image(systemName: icon).font(.title3)
                Text(label).font(.small).lineLimit(1)
            }
            .foregroundStyle(Color.ozeroAccent)
            .frame(maxWidth: .infinity, minHeight: 64)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .glassSurface(in: .rect(cornerRadius: 20), fallback: Color.ozeroCard)
    }
}

/// All mail from or to someone, newest first, under their name; it stays current as mail comes.
struct PersonMailView: View {
    let model: InboxModel
    let person: PersonCard

    @State private var messages: [MessageItem]?
    @State private var error: LocalizedStringKey?

    var body: some View {
        List {
            if let error {
                Text(error).font(.small).foregroundStyle(.secondary).listRowSeparator(.hidden)
            }
            if messages?.isEmpty == true {
                ContentUnavailableView("inbox_empty", systemImage: "tray")
                    .listRowSeparator(.hidden)
            }
            ForEach(messages ?? [], id: \.id) { message in
                OpenRow(model: model, message: message)
            }
        }
        .listStyle(.plain)
        .middle(Room.list)
        .navigationTitle(Text(verbatim: person.name))
        .navigationBarTitleDisplayMode(.large)
        .task(id: model.revision) {
            do {
                messages = try await model.mail.mail(with: person.emails)
                error = nil
            } catch {
                self.error = LocalizedStringKey(error.textKey())
            }
        }
    }
}
