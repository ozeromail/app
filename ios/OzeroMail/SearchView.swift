import SwiftUI

/// Search over the saved mail, newest first, as the user types; once typing stops, the servers
/// are asked too and what they find joins the list. The field sits at the bottom, in reach of the
/// thumb; Close or back returns to the inbox.
struct SearchView: View {
    let model: InboxModel

    @State private var query = ""
    @Environment(\.sideways) private var sideways
    /// With the query they answer, so "No messages" never shows for a query still waiting.
    @State private var found: (query: String, messages: [MessageItem])?
    @State private var error: LocalizedStringKey?
    /// The servers are being asked about the query on screen.
    @State private var asking = false
    @FocusState private var focused: Bool

    var body: some View {
        let messages = found?.messages ?? []
        List {
            if found?.query == query && !query.isBlank && messages.isEmpty && !asking {
                ContentUnavailableView("inbox_empty", systemImage: "magnifyingglass")
                    .listRowSeparator(.hidden)
            }
            // No motion in this list: when the servers' answer joins it, rows sliding to their new
            // places would pass over the rows that appear at once.
            ForEach(messages, id: \.id) { message in
                NavigationLink(value: Route.message(message)) { MessageRow(message: message) }
            }
            if asking {
                Text("search_server")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
                    .listRowSeparator(.hidden)
            }
        }
        .listStyle(.plain)
        .middle(sideways ? Room.list : .infinity)
        .navigationBarTitleDisplayMode(.inline)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            VStack(spacing: 8) {
                if let error {
                    Text(error).font(.footnote).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
                }
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
                    .background(Color.ozeroCard, in: .capsule)
                    Button {
                        model.path.removeAll { $0 == .search }
                    } label: {
                        Image(systemName: "xmark").foregroundStyle(.primary).frame(width: 48, height: 48).background(Color.ozeroCard, in: .circle)
                    }
                    .accessibilityLabel(Text("action_close"))
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(.bar)
        }
        // Not on the way back from a result: the keyboard would cover the results.
        .onAppear { if query.isEmpty { focused = true } }
        .task(id: query) {
            error = nil
            let text = query
            guard !text.isBlank else {
                found = nil
                return
            }
            do {
                // A pause in typing starts a search, so each key does not.
                try await Task.sleep(for: .milliseconds(150))
                found = (text, try await model.mail.search(text))
                // A longer pause also asks the servers, for mail the phone does not have.
                try await Task.sleep(for: .milliseconds(550))
                asking = true
                defer { asking = false }
                do {
                    try await model.mail.searchServer(text)
                } catch is CancellationError {
                    throw CancellationError()
                } catch {
                    // What the servers found so far still shows.
                    self.error = error.text()
                }
                found = (text, try await model.mail.search(text))
            } catch is CancellationError {
                // The query changed meanwhile.
            } catch {
                self.error = error.text()
            }
        }
    }
}
