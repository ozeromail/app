import QuickLook
import SwiftUI
import UniformTypeIdentifiers

/// The attachments of the open conversation: downloads, the viewer, and what is shared or saved.
@MainActor @Observable
final class Files {
    /// Downloads under way of each file, by message and part.
    var loading: [String: Int] = [:]
    /// What the viewer pages through, and where it starts.
    var preview: [URL] = []
    var previewAt: URL?
    /// Files to share or open in another app.
    var sharing: [URL]?
    /// Files to save to Files.
    var saving: [URL] = []
    var exporting = false
    var error: LocalizedStringKey?
    var note: LocalizedStringKey?

    private func key(_ message: MessageItem, _ file: AttachmentItem) -> String { "\(message.id)-\(file.part)" }

    func isLoading(_ message: MessageItem, _ file: AttachmentItem) -> Bool {
        loading[key(message, file), default: 0] > 0
    }

    /// Downloads `items` once, together, then hands their files to `done`.
    private func download(_ mail: Mail, _ message: MessageItem, _ items: [AttachmentItem], _ done: @escaping ([URL]) -> Void) {
        let keys = items.map { key(message, $0) }
        for key in keys { loading[key, default: 0] += 1 }
        error = nil
        Task {
            defer { for key in keys { loading[key, default: 1] -= 1 } }
            do {
                done(try await mail.attachments(of: message, items))
            } catch {
                self.error = error.text()
            }
        }
    }

    /// Opens `file` in the viewer as soon as it is here; the other files of its message join it
    /// to page through once they are too.
    func open(_ mail: Mail, _ message: MessageItem, _ file: AttachmentItem, all: [AttachmentItem]) {
        download(mail, message, [file]) { urls in
            guard let url = urls.first else { return }
            self.preview = [url]
            self.previewAt = url
            guard all.count > 1 else { return }
            Task {
                guard let every = try? await mail.attachments(of: message, all), self.previewAt == url else { return }
                self.preview = every
            }
        }
    }

    func share(_ mail: Mail, _ message: MessageItem, _ items: [AttachmentItem]) {
        download(mail, message, items) { self.sharing = $0 }
    }

    func save(_ mail: Mail, _ message: MessageItem, _ items: [AttachmentItem]) {
        download(mail, message, items) { urls in
            self.saving = urls
            self.exporting = true
        }
    }
}

/// The files sent with a message, under it: a row each, and Save all for more than one.
struct AttachmentList: View {
    let mail: Mail
    let files: Files
    let message: MessageItem
    let items: [AttachmentItem]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("message_attachments").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                Spacer()
                if items.count > 1 {
                    Button("attachment_save_all") { files.save(mail, message, items) }
                        .font(.footnote.weight(.semibold))
                }
            }
            .padding(.horizontal, 6)
            VStack(spacing: 0) {
                ForEach(items, id: \.part) { file in
                    AttachmentRow(mail: mail, files: files, message: message, file: file, all: items)
                }
            }
            .background(Color.ozeroCard, in: .rect(cornerRadius: 16))
            if let error = files.error ?? files.note {
                Text(error).font(.footnote).foregroundStyle(.secondary).padding(.horizontal, 6)
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 10)
        .padding(.bottom, 6)
    }
}

private struct AttachmentRow: View {
    let mail: Mail
    let files: Files
    let message: MessageItem
    let file: AttachmentItem
    let all: [AttachmentItem]

    var body: some View {
        HStack(spacing: 12) {
            Button {
                files.open(mail, message, file, all: all)
            } label: {
                HStack(spacing: 12) {
                    ZStack {
                        RoundedRectangle(cornerRadius: 8).fill(Color(uiColor: .systemBackground)).frame(width: 40, height: 40)
                        if files.isLoading(message, file) {
                            ProgressView()
                        } else if let kind = file.kind {
                            Text(verbatim: kind).font(.caption2.weight(.bold)).foregroundStyle(.secondary)
                        } else {
                            Image(systemName: "doc").foregroundStyle(.secondary)
                        }
                    }
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: file.title).font(.subheadline.weight(.medium)).lineLimit(1).truncationMode(.middle)
                        Text(verbatim: file.details).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            Menu {
                Button("attachment_open", systemImage: "eye") { files.open(mail, message, file, all: all) }
                Button("attachment_share", systemImage: "square.and.arrow.up") { files.share(mail, message, [file]) }
                Button("attachment_save_files", systemImage: "folder") { files.save(mail, message, [file]) }
            } label: {
                Image(systemName: "ellipsis").foregroundStyle(.secondary).frame(width: 40, height: 40)
            }
            .accessibilityLabel(Text("action_more"))
        }
        .padding(.leading, 12)
        .padding(.trailing, 4)
        .padding(.vertical, 8)
    }
}

extension View {
    /// The viewer, the share sheet and saving to Files for a conversation's attachments.
    func attachmentActions(_ files: Files) -> some View {
        modifier(AttachmentActions(files: files))
    }
}

private struct AttachmentActions: ViewModifier {
    @Bindable var files: Files

    func body(content: Content) -> some View {
        content
            .quickLookPreview($files.previewAt, in: files.preview)
            .sheet(isPresented: Binding(get: { files.sharing != nil }, set: { if !$0 { files.sharing = nil } })) {
                ShareSheet(items: files.sharing ?? []).presentationDetents([.medium, .large])
            }
            .sheet(isPresented: $files.exporting) {
                ExportSheet(urls: files.saving) { saved in files.note = saved ? "attachment_saved_files" : nil }
                    .ignoresSafeArea()
            }
    }
}

/// The system's picker of where in Files to save copies of `urls`.
private struct ExportSheet: UIViewControllerRepresentable {
    let urls: [URL]
    let onDone: (Bool) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(onDone: onDone) }

    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let picker = UIDocumentPickerViewController(forExporting: urls, asCopy: true)
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: UIDocumentPickerViewController, context: Context) {}

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        let onDone: (Bool) -> Void

        init(onDone: @escaping (Bool) -> Void) {
            self.onDone = onDone
        }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            onDone(true)
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            onDone(false)
        }
    }
}

/// The system's share sheet, which also opens a file in another app.
private struct ShareSheet: UIViewControllerRepresentable {
    let items: [URL]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

extension AttachmentItem {
    /// A file's name as sent, or a word for it when it came without one.
    var title: String { name.isBlank ? tr("attachment_untitled") : name }

    /// The kind at a glance, "PDF", from the name or else the type; nil when neither tells.
    var kind: String? {
        let ext = (name as NSString).pathExtension
        if !ext.isEmpty && ext.count <= 4 { return ext.uppercased() }
        return UTType(mimeType: mime)?.preferredFilenameExtension?.uppercased()
    }

    /// "PDF · 84 KB": the kind, then the size.
    var details: String {
        let size = ByteCountFormatter.string(fromByteCount: Int64(size), countStyle: .file)
        return kind.map { "\($0) · \(size)" } ?? size
    }
}
