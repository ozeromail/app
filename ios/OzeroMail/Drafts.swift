import SwiftUI

/// The drafts on the phone, last saved first, in rows like mail's; a tap opens one to go on
/// writing, and the swipe that deletes mail throws one away, which the toast can bring back.
struct DraftsView: View {
    let model: InboxModel

    var body: some View {
        // Only Delete applies to a draft: on the side whose swipes delete mail, else to the left.
        let swipes = model.swipes
        let right = swipes.right == .delete || swipes.rightLong == .delete
        let left = swipes.left == .delete || swipes.leftLong == .delete || !right
        List {
            if model.drafts.isEmpty {
                ContentUnavailableView("drafts_empty", systemImage: "square.and.pencil")
                    .listRowSeparator(.hidden)
            }
            ForEach(model.drafts, id: \.draft.id) { row in
                Button { model.openDraft(row.draft) } label: { DraftLine(row: row) }
                    .buttonStyle(.plain)
                    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                        if left { delete(row.draft) }
                    }
                    .swipeActions(edge: .leading, allowsFullSwipe: true) {
                        if right { delete(row.draft) }
                    }
            }
        }
        .listStyle(.plain)
        .animation(.smooth, value: model.drafts.map(\.draft.id))
        .middle(Room.list)
        .navigationTitle("folder_drafts")
        .safeAreaInset(edge: .bottom, spacing: 0) {
            // The list's toast shows here too, so a draft thrown away can come back.
            VStack {
                if let toast = model.toast {
                    ToastView(toast: toast, onUndo: model.tapUndo, onGo: model.tapGo, onExtra: model.tapExtra)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .padding(.bottom, 8)
            .animation(.smooth, value: model.toast)
        }
        .task { await model.counts() }
    }

    private func delete(_ draft: DraftItem) -> some View {
        Button(role: .destructive) {
            model.deleteDraft(draft)
        } label: {
            Label(SwipeAction.delete.label, systemImage: SwipeAction.delete.icon)
        }
        .tint(SwipeAction.delete.tint)
    }
}

/// As a row of mail: who it goes to, its account and when it was saved, then the subject, the
/// start of the text and its files.
private struct DraftLine: View {
    let row: DraftRow

    var body: some View {
        let draft = row.draft
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(verbatim: row.people.isBlank ? tr("drafts_no_recipients") : row.people)
                    .font(.body.weight(.medium))
                    .lineLimit(1)
                Spacer(minLength: 4)
                if let account = row.account { AccountTag(account: account) }
                Text(verbatim: shortDate(draft.saved))
                    .font(.footnote.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            Text(verbatim: draft.subject.isBlank ? tr("inbox_no_subject") : draft.subject)
                .font(.subheadline)
                .lineLimit(1)
            if !row.snippet.isBlank {
                Text(verbatim: row.snippet)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            if !draft.files.isEmpty {
                AttachmentChips(names: draft.files.map(\.name))
            }
        }
        .padding(.vertical, 6)
        .contentShape(.rect)
    }
}
