import CoreTransferable
import SwiftUI
import UniformTypeIdentifiers

/// A file picked to go with a message: its name, type and content.
struct PickedFile: Sendable {
    let name: String
    let mime: String
    let data: Data

    /// The file at `url`, read whole if it takes at most `room` bytes; nil if it takes more.
    static func read(_ url: URL, room: UInt64) throws -> PickedFile? {
        let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
        guard UInt64(size) <= room else { return nil }
        let data = try Data(contentsOf: url, options: .mappedIfSafe)
        // The size the system gives may be wrong.
        guard UInt64(data.count) <= room else { return nil }
        let mime = UTType(filenameExtension: url.pathExtension)?.preferredMIMEType ?? "application/octet-stream"
        return PickedFile(name: url.lastPathComponent, mime: mime, data: data)
    }
}

/// A photo or video from the library, read as the picker hands it over; nil when it takes more
/// than a message carries.
struct PickedMedia: Transferable {
    let file: PickedFile?

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .movie, importing: received)
        FileRepresentation(importedContentType: .image, importing: received)
    }

    /// Read at once: the picker takes its copy away afterwards.
    private static func received(_ received: ReceivedTransferredFile) throws -> PickedMedia {
        PickedMedia(file: try PickedFile.read(received.file, room: fileLimit()))
    }
}

/// The files added to a message being written, each with a way to take it off; `adding` are
/// the names of those still being read, shown faint.
struct FileChips: View {
    let files: [DraftFileItem]
    let adding: [String]
    let onRemove: (DraftFileItem) -> Void

    var body: some View {
        Flow(spacing: 6) {
            ForEach(files, id: \.id) { file in
                FileChip(name: file.name, size: file.size, onRemove: { onRemove(file) })
            }
            ForEach(Array(adding.enumerated()), id: \.offset) { _, name in
                FileChip(name: name, size: nil, onRemove: nil)
            }
        }
    }
}

/// Lays its views out in rows, left to right, starting a new row where one does not fit.
struct Flow: Layout {
    let spacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let frames = frames(subviews, width: proposal.width ?? .infinity)
        return CGSize(width: frames.map(\.maxX).max() ?? 0, height: frames.map(\.maxY).max() ?? 0)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for (view, frame) in zip(subviews, frames(subviews, width: bounds.width)) {
            view.place(at: CGPoint(x: bounds.minX + frame.minX, y: bounds.minY + frame.minY),
                       proposal: ProposedViewSize(frame.size))
        }
    }

    /// Where each view goes in rows `width` wide; one wider than a row gets the row.
    private func frames(_ subviews: Subviews, width: CGFloat) -> [CGRect] {
        var frames: [CGRect] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var row: CGFloat = 0
        for view in subviews {
            var size = view.sizeThatFits(.unspecified)
            size.width = min(size.width, width)
            if x > 0 && x + size.width > width {
                x = 0
                y += row + spacing
                row = 0
            }
            frames.append(CGRect(origin: CGPoint(x: x, y: y), size: size))
            x += size.width + spacing
            row = max(row, size.height)
        }
        return frames
    }
}

/// A file's name and size; while `size` is nil it is being read.
private struct FileChip: View {
    let name: String
    let size: UInt64?
    let onRemove: (() -> Void)?

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "paperclip").foregroundStyle(.secondary)
            Text(verbatim: attachmentTitle(name))
                .lineLimit(1)
                .truncationMode(.middle)
            if let size {
                Text(verbatim: ByteCountFormatter.string(fromByteCount: Int64(clamping: size), countStyle: .file))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if let onRemove {
                Button(action: onRemove) { Image(systemName: "xmark").foregroundStyle(.secondary) }
                    .frame(width: 32, height: 32)
                    .accessibilityLabel(Text("action_remove"))
            }
        }
        .font(.footnote)
        .padding(.leading, 8)
        .padding(.trailing, onRemove == nil ? 10 : 0)
        .frame(minHeight: 32)
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(.quaternary))
        .frame(maxWidth: 240, alignment: .leading)
        .opacity(size == nil ? 0.5 : 1)
    }
}
