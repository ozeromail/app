import SwiftUI

/// Who a message goes to, as one field: the people picked, each a chip with their name, then the
/// address being typed. `text` keeps them as typed addresses, comma apart. While an address is
/// typed, the people it may be rise over the field, the likeliest nearest to it and the thumb;
/// a tap on one takes it.
struct Recipients<Field: Hashable>: View {
    let mail: Mail
    @Binding var text: String
    let placeholder: LocalizedStringKey
    let focus: FocusState<Field?>.Binding
    let field: Field

    /// The names of the people picked here, by address; others show as their address.
    @State private var names: [String: String] = [:]
    /// The people found on the phone and in the directories, with what they were found for.
    @State private var local: Found = Found()
    @State private var directory: Found = Found()
    /// Counts the times the contacts came, so the people are looked up again with them.
    @State private var contactsGiven = 0
    @State private var offerContacts = PhoneContacts.askable
    /// Room over the field for the people; else they go under it.
    @State private var above = true
    @State private var listHeight: CGFloat = 0

    var body: some View {
        let parts = Typed.of(text)
        let typing = parts.typing.trimmingCharacters(in: .whitespacesAndNewlines)
        VStack(alignment: .leading, spacing: 6) {
            if !parts.done.isEmpty {
                Flow(spacing: 6) {
                    ForEach(parts.done, id: \.self) { email in
                        RecipientChip(name: names[email] ?? email) {
                            text = Typed(done: parts.done.filter { $0 != email }, typing: parts.typing).text
                        }
                    }
                }
            }
            // A comma, a semicolon or a space after an address makes it a chip.
            TextField(parts.done.isEmpty ? placeholder : "", text: Binding(
                get: { parts.typing },
                set: { text = Typed.of(Typed(done: parts.done, typing: "").prefix + $0).text }
            ))
            .keyboardType(.emailAddress)
            .textContentType(.emailAddress)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .focused(focus, equals: field)
        }
        .onGeometryChange(for: Bool.self) { $0.frame(in: .scrollView).minY > Self.maxHeight + 8 } action: { above = $0 }
        .overlay(alignment: above ? .top : .bottom) {
            if focus.wrappedValue == field && !typing.isEmpty {
                suggestions(for: typing, done: parts.done)
                    .alignmentGuide(above ? .top : .bottom) { above ? $0[.bottom] + 8 : $0[.top] - 8 }
            }
        }
        .task(id: parts.done) {
            // Addresses that came with the draft get the name the mail or the contacts give them.
            let unnamed = parts.done.filter { names[$0] == nil }
            guard !unnamed.isEmpty, let found = try? await mail.recipientNames(unnamed) else { return }
            for (email, name) in zip(unnamed, found) { names[email] = name }
        }
        .task(id: "\(contactsGiven) \(typing)") {
            guard !typing.isEmpty else { return }
            try? await Task.sleep(for: .milliseconds(60))
            guard !Task.isCancelled else { return }
            local = Found(text: typing, people: (try? await mail.findPeople(typing)) ?? [])
        }
        .task(id: typing) {
            guard !typing.isEmpty else { return }
            // A longer pause asks the company directories too, which answer from three letters.
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            // Offline or refused: the people on the phone are enough.
            directory = Found(text: typing, people: (try? await mail.searchDirectory(typing)) ?? [])
        }
    }

    private static var maxHeight: CGFloat { 320 }

    @ViewBuilder
    private func suggestions(for typing: String, done: [String]) -> some View {
        let shown = local.text == typing ? local.people : []
        let listed = (directory.text == typing ? directory.people : [])
            .filter { found in !shown.contains { $0.email.caseInsensitiveCompare(found.email) == .orderedSame } }
        if !shown.isEmpty || !listed.isEmpty || offerContacts {
            // The likeliest person nearest the field, whichever side the list is on.
            let people: [Row] = shown.map { .person($0) }
            let others: [Row] = listed.map { .person($0) }
            let header: [Row] = listed.isEmpty ? [] : [.header]
            let ask: [Row] = offerContacts ? [.ask] : []
            let nearestFirst = people + (above ? others + header : header + others) + ask
            let rows: [Row] = above ? Array(nearestFirst.reversed()) : nearestFirst
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(rows) { row in
                        switch row {
                        case .person(let person): PersonRow(person: person) { pick(person, done: done) }
                        case .header:
                            Text("compose_directory")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 16)
                                .padding(.top, 12)
                                .padding(.bottom, 2)
                        case .ask: askRow
                        }
                    }
                }
                .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { listHeight = $0 }
            }
            .defaultScrollAnchor(above ? .bottom : .top)
            .frame(height: min(listHeight, Self.maxHeight))
            .background(Color(.systemBackground), in: .rect(cornerRadius: 14))
            .overlay { RoundedRectangle(cornerRadius: 14).stroke(Color(.separator), lineWidth: 0.5) }
            .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
        }
    }

    private var askRow: some View {
        Button {
            offerContacts = false
            Task {
                guard await PhoneContacts.ask() else { return }
                await mail.giveContacts()
                contactsGiven += 1
            }
        } label: {
            Label("compose_contacts_ask", systemImage: "person.crop.circle")
                .foregroundStyle(Color.ozeroAccent)
                .frame(maxWidth: .infinity, minHeight: 48, alignment: .leading)
                .padding(.horizontal, 16)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }

    private func pick(_ person: PersonFound, done: [String]) {
        names[person.email] = person.name
        text = Typed(done: done + [person.email], typing: "").text
    }

    private struct Found {
        var text = ""
        var people: [PersonFound] = []
    }

    private enum Row: Identifiable {
        case person(PersonFound)
        case header
        case ask

        var id: String {
            switch self {
            case .person(let person): "p" + person.email
            case .header: "header"
            case .ask: "ask"
            }
        }
    }
}

/// Someone to write to: their name, and the address with their title, if a directory gave one.
private struct PersonRow: View {
    let person: PersonFound
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: person.name).lineLimit(1)
                Text(verbatim: [person.email == person.name ? nil : person.email, person.title].compactMap { $0 }.joined(separator: " · "))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .padding(.vertical, 9)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

/// Someone picked, by name; the cross takes them off.
private struct RecipientChip: View {
    let name: String
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            Text(verbatim: name).lineLimit(1)
            Button(action: onRemove) {
                Image(systemName: "xmark")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .frame(width: 32, height: 32)
                    .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(tr("compose_remove_person", name))
        }
        .padding(.leading, 12)
        .frame(maxWidth: 240, alignment: .leading)
        .background(Color.ozeroCard, in: .capsule)
    }
}

/// A field of addresses: those `done`, each one whole, then what is being `typing`.
struct Typed: Equatable {
    var done: [String]
    var typing: String

    /// The field as kept in the draft.
    var text: String { prefix + typing }

    /// What goes before the address being typed.
    var prefix: String { done.map { $0 + ", " }.joined() }

    /// Splits `text` where a comma or a semicolon ends an address, or white space after a whole
    /// one: a name being typed keeps its spaces.
    static func of(_ text: String) -> Typed {
        let segments = text.split(omittingEmptySubsequences: false) { $0 == "," || $0 == ";" || $0 == "\n" }
        var done = segments.dropLast().map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
        var typing = Substring(segments.last ?? "").drop { $0.isWhitespace }
        while let whole = typing.firstMatch(of: #/^(\S+@\S+)\s+/#) {
            done.append(String(whole.1))
            typing = typing[whole.range.upperBound...]
        }
        var seen = Set<String>()
        return Typed(done: done.filter { seen.insert($0.lowercased()).inserted }, typing: String(typing))
    }
}
