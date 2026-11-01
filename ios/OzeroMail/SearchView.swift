import SwiftUI

/// Search over the saved mail, newest first, as the user types; once typing stops, the servers
/// are asked too and what they find joins the list. The field sits at the bottom, in reach of the
/// thumb, with filters over it that narrow what the words find, or list all they keep with no
/// words; Close or back returns to the inbox. On iOS 26 the field is the system's, in glass at
/// the bottom, and closing it returns to the inbox too. Before anything is typed, recent searches and the
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
    /// The system's field is open, on iOS 26.
    @State private var presented = false
    /// On screen, not under a result opened from it.
    @State private var visible = false

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
        .bottomBar {
            VStack(spacing: 8) {
                if let error {
                    Text(error).font(.small).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 12)
                }
                SearchFilters(filter: $filter, accounts: model.accounts)
                if !hasGlass {
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
                            Image(systemName: "xmark").foregroundStyle(.primary).frame(width: 48, height: 48).contentShape(.circle)
                        }
                        .buttonStyle(.plain)
                        .background(Color.ozeroCard, in: .circle)
                        .accessibilityLabel(Text("action_close"))
                    }
                    .padding(.horizontal, 12)
                }
            }
            .padding(.top, 6)
            .padding(.bottom, 10)
            .dockBackground(floating: true)
        }
        .systemSearch($query, presented: $presented)
        // Not on the way back from a result: the keyboard would cover the results.
        .onAppear {
            visible = true
            guard search.isEmpty else { return }
            if hasGlass { presented = true } else { focused = true }
        }
        .onDisappear { visible = false }
        // The system field's close button closes search.
        .onChange(of: presented) { was, now in
            // Not when the field closes for a result or a person opened over search.
            if was, !now, visible, model.path.last == .search { model.path.removeAll { $0 == .search } }
        }
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

extension View {
    /// On iOS 26, the system's search field, at the bottom in glass; before, the screen's own.
    @ViewBuilder
    fileprivate func systemSearch(_ text: Binding<String>, presented: Binding<Bool>) -> some View {
        if #available(iOS 26, *) {
            searchable(text: text, isPresented: presented, prompt: Text("search_prompt"))
                .autocorrectionDisabled()
        } else {
            self
        }
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
                        .padding(.horizontal, 12)
                    }
                    .padding(.bottom, 2)
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

/// A person to tap for all mail with them, as a chip of one line: the first name, then the rest of it.
private struct PersonTile: View {
    let person: PersonFound
    let onTap: () -> Void

    var body: some View {
        let words = person.name.split(separator: " ", maxSplits: 1)
        let first = Text(verbatim: words.first.map(String.init) ?? person.name).font(.text.weight(.medium))
        let rest = words.count > 1 ? Text(verbatim: " " + words[1]).font(.text).foregroundStyle(.secondary) : Text(verbatim: "")
        Button(action: onTap) {
            Text("\(first)\(rest)")
                .lineLimit(1)
                .chip(Color.ozeroCard)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(verbatim: person.name))
    }
}

/// A filter in [SearchFilters]: its key, what it says, whether it is on, its mark and what a tap does.
private struct Filter: Identifiable {
    enum Mark {
        case symbol(String)
        case dot(Color)
    }

    let id: String
    let title: Text
    let on: Bool
    let mark: Mark
    let toggle: () -> Void
}

/// The row of filters over the field: unread, flagged, with attachments and, with several
/// accounts, one for each of them; an account's filter takes the place of another's. The ones
/// on come first, so what narrows the search is always in sight.
private struct SearchFilters: View {
    @Binding var filter: SearchFilterItem
    let accounts: [AccountItem]

    private var all: [Filter] {
        var list = [
            Filter(id: "unread", title: Text("folder_unread"), on: filter.unread, mark: .symbol("envelope.badge")) { filter.unread.toggle() },
            Filter(id: "flagged", title: Text("folder_flagged"), on: filter.flagged, mark: .symbol("flag")) { filter.flagged.toggle() },
            Filter(id: "files", title: Text("folder_files"), on: filter.files, mark: .symbol("paperclip")) { filter.files.toggle() },
        ]
        if accounts.count > 1 {
            for account in accounts {
                let on = filter.accountId == account.id
                list.append(Filter(id: "account-\(account.id)", title: Text(verbatim: account.label.isBlank ? account.email : account.label), on: on, mark: .dot(account.tint)) {
                    filter.accountId = on ? nil : account.id
                })
            }
        }
        // A stable sort: the ones on keep their order among themselves, and so do the rest.
        return list.filter(\.on) + list.filter { !$0.on }
    }

    var body: some View {
        let items = all
        ScrollViewReader { reader in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(items) { item in
                        chip(item).id(item.id)
                    }
                }
                .padding(.horizontal, 12)
            }
            // A chip turned on moves to the front: bring the front into view with it.
            .onChange(of: filter) {
                if let first = items.first?.id { reader.scrollTo(first, anchor: .leading) }
            }
        }
        .animation(.smooth, value: filter)
    }

    private func chip(_ item: Filter) -> some View {
        Button(action: item.toggle) {
            HStack(spacing: 7) {
                switch item.mark {
                case .symbol(let name): Image(systemName: name).font(.footnote)
                case .dot(let color): AccountDot(color: color, size: 8)
                }
                item.title.font(.text.weight(.medium)).lineLimit(1)
            }
            .foregroundStyle(item.on ? Color.white : .primary)
            .chip(item.on ? Color.ozeroFill : Color.ozeroCard)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(item.on ? .isSelected : [])
    }
}

private extension View {
    /// The pill every chip on the search screen sits in, people and filters alike: one height, one shape.
    func chip(_ fill: Color) -> some View {
        padding(.horizontal, 13)
            .frame(height: 34)
            .frame(maxWidth: 200)
            .background(fill, in: .capsule)
    }
}
