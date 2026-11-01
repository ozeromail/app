import SwiftUI

/// Search over the saved mail, newest first, as the user types; once typing stops, the servers
/// are asked too and what they find joins the list. The field sits at the bottom, in reach of the
/// thumb, with filters over it that narrow what the words find, or list all they keep with no
/// words; Close or back returns to the inbox. Before anything is typed, recent searches and the
/// people the user has most mail with grow up from the field.
struct SearchView: View {
    let model: InboxModel

    @State private var query = ""
    @State private var filter = SearchFilterItem()
    /// With the search they answer, so "No messages" never shows for a search still waiting.
    @State private var found: (search: Search, messages: [MessageItem])?
    @State private var error: LocalizedStringKey?
    /// The servers are being asked about the query on screen.
    @State private var asking = false
    @State private var recent: [String] = []
    @State private var people: [PersonFound] = []
    @FocusState private var focused: Bool

    private var search: Search { Search(text: query, filter: asked) }

    /// An account's filter goes with the account, or with its chip once one account is left.
    private var asked: SearchFilterItem {
        guard let id = filter.accountId, model.accounts.count < 2 || !model.accounts.contains(where: { $0.id == id }) else {
            return filter
        }
        var asked = filter
        asked.accountId = nil
        return asked
    }

    var body: some View {
        Group {
            if search.isEmpty {
                SearchStart(
                    recent: recent,
                    people: people,
                    onSearch: { query = $0 },
                    onPerson: { person in
                        Task {
                            do {
                                model.path.append(.person(try await model.mail.person(email: person.email, name: person.name)))
                            } catch {
                                self.error = error.text()
                            }
                        }
                    },
                    onForget: {
                        recent = []
                        Task { try? await model.mail.forgetSearches() }
                    })
            } else {
                results
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            VStack(spacing: 8) {
                if let error {
                    Text(error).font(.small).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 12)
                }
                SearchFilters(filter: $filter, accounts: model.accounts)
                HStack(spacing: 8) {
                    HStack {
                        Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                        TextField("search_prompt", text: $query)
                            .focused($focused)
                            .submitLabel(.search)
                            .autocorrectionDisabled()
                    }
                    .padding(.horizontal, 16)
                    .frame(height: 48)
                    .background(Color.ozeroCard.sunk, in: .capsule)
                    Button {
                        focused = false
                        model.path.removeAll { $0 == .search }
                    } label: {
                        Image(systemName: "xmark").foregroundStyle(.primary).frame(width: 48, height: 48).background(Color.ozeroCard, in: .circle)
                    }
                    .accessibilityLabel(Text("action_close"))
                }
                .padding(.horizontal, 12)
            }
            .padding(.top, 6)
            .padding(.bottom, 10)
            .background(.bar)
        }
        // Not on the way back from a result: the keyboard would cover the results.
        .onAppear { if search.isEmpty { focused = true } }
        .task {
            if let recent = try? await model.mail.recentSearches() { self.recent = recent }
            if let people = try? await model.mail.frequentPeople() { self.people = people }
        }
        .task(id: search) {
            error = nil
            let search = search
            guard !search.isEmpty else {
                found = nil
                return
            }
            do {
                // A pause in typing starts a search, so each key does not.
                if !search.text.isBlank { try await Task.sleep(for: .milliseconds(150)) }
                found = (search, try await model.mail.search(search.text, filter: search.filter))
                // Filters alone list saved mail only: the servers search by words.
                guard !search.text.isBlank else { return }
                // A longer pause also asks the servers, for mail the phone does not have.
                try await Task.sleep(for: .milliseconds(550))
                asking = true
                defer { asking = false }
                do {
                    try await model.mail.searchServer(search.text)
                } catch is CancellationError {
                    throw CancellationError()
                } catch {
                    // What the servers found so far still shows.
                    self.error = error.text()
                }
                found = (search, try await model.mail.search(search.text, filter: search.filter))
            } catch is CancellationError {
                // The search changed meanwhile.
            } catch {
                self.error = error.text()
            }
        }
    }

    private var results: some View {
        let messages = found?.messages ?? []
        return List {
            if found?.search == search && messages.isEmpty && !asking {
                ContentUnavailableView("inbox_empty", systemImage: "magnifyingglass")
                    .listRowSeparator(.hidden)
            }
            // No motion in this list: when the servers' answer joins it, rows sliding to their new
            // places would pass over the rows that appear at once.
            ForEach(messages, id: \.id) { message in
                OpenRow(model: model, message: message)
                    .simultaneousGesture(TapGesture().onEnded {
                        let text = query
                        guard !text.isBlank else { return }
                        Task { try? await model.mail.rememberSearch(text) }
                    })
            }
            if asking {
                Text("search_server")
                    .font(.small)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
                    .listRowSeparator(.hidden)
            }
        }
        .listStyle(.plain)
        .middle(Room.list)
    }
}

/// The words and the filters of one search.
private struct Search: Equatable {
    let text: String
    let filter: SearchFilterItem

    var isEmpty: Bool { text.isBlank && filter == SearchFilterItem() }
}

/// What the screen shows before anything is typed, growing up from the field: recent searches
/// nearest the thumb, the people the user has most mail with over them.
private struct SearchStart: View {
    let recent: [String]
    let people: [PersonFound]
    let onSearch: (String) -> Void
    let onPerson: (PersonFound) -> Void
    let onForget: () -> Void

    var body: some View {
        // Anchored at the bottom, so the column starts at the field and a long one keeps its end there.
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                if !people.isEmpty {
                    caption("search_people")
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(people, id: \.email) { person in
                                PersonTile(person: person) { onPerson(person) }
                            }
                        }
                        .padding(.horizontal, 20)
                    }
                    .padding(.bottom, 8)
                }
                if !recent.isEmpty {
                    caption("search_recent") {
                        Button("search_clear", action: onForget)
                            .font(.smallStrong)
                            .foregroundStyle(Color.ozeroAccent)
                    }
                    ForEach(recent, id: \.self) { text in
                        Button { onSearch(text) } label: {
                            HStack(spacing: 14) {
                                Image(systemName: "magnifyingglass").foregroundStyle(.secondary).frame(width: 24)
                                Text(verbatim: text).font(.rowPlain).lineLimit(1)
                                Spacer(minLength: 0)
                            }
                            .padding(.horizontal, 20)
                            .frame(minHeight: 48)
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .defaultScrollAnchor(.bottom)
        .scrollBounceBehavior(.basedOnSize)
        .scrollDismissesKeyboard(.interactively)
    }

    private func caption(_ key: LocalizedStringKey, @ViewBuilder action: () -> some View = { EmptyView() }) -> some View {
        HStack {
            Text(key).font(.smallStrong).foregroundStyle(.secondary)
            Spacer()
            action()
        }
        .padding(.horizontal, 20)
        .frame(minHeight: 36)
    }
}

/// A person to tap for all mail with them: the first name over the rest of it.
private struct PersonTile: View {
    let person: PersonFound
    let onTap: () -> Void

    var body: some View {
        let words = person.name.split(separator: " ", maxSplits: 1)
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: words.first.map(String.init) ?? person.name).font(.text.weight(.medium)).lineLimit(1)
                // A blank line keeps every tile the same height.
                Text(verbatim: words.count > 1 ? String(words[1]) : " ").font(.label).foregroundStyle(.secondary).lineLimit(1)
            }
            .frame(maxWidth: 160, alignment: .leading)
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .background(Color.ozeroCard, in: .rect(cornerRadius: 14))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(verbatim: person.name))
    }
}

/// The row of filters over the field: unread, flagged, with attachments and, with several
/// accounts, one for each of them; an account's filter takes the place of another's.
private struct SearchFilters: View {
    @Binding var filter: SearchFilterItem
    let accounts: [AccountItem]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                chip(Text("folder_unread"), on: filter.unread) { Image(systemName: "circle.fill").font(.system(size: 7)) } toggle: {
                    filter.unread.toggle()
                }
                chip(Text("folder_flagged"), on: filter.flagged) { Image(systemName: "flag") } toggle: {
                    filter.flagged.toggle()
                }
                chip(Text("folder_files"), on: filter.files) { Image(systemName: "paperclip") } toggle: {
                    filter.files.toggle()
                }
                if accounts.count > 1 {
                    ForEach(accounts, id: \.id) { account in
                        let on = filter.accountId == account.id
                        chip(Text(verbatim: account.label.isBlank ? account.email : account.label), on: on) {
                            AccountDot(color: account.tint, size: 8)
                        } toggle: {
                            filter.accountId = on ? nil : account.id
                        }
                    }
                }
            }
            .padding(.horizontal, 12)
        }
    }

    private func chip(
        _ title: Text, on: Bool,
        @ViewBuilder mark: () -> some View, toggle: @escaping () -> Void
    ) -> some View {
        Button(action: toggle) {
            HStack(spacing: 7) {
                mark().font(.footnote)
                title.font(.text.weight(.medium)).lineLimit(1)
            }
            .foregroundStyle(on ? Color.ozeroAccent : .primary)
            .padding(.horizontal, 13)
            .frame(height: 34)
            .frame(maxWidth: 200)
            .background(on ? Color.ozeroAccent.opacity(0.14) : Color.ozeroCard, in: .capsule)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}
