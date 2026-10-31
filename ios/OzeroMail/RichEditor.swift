import CoreText
import SwiftUI
import UIKit

/// The text of a message being written, with its styles and images: text views between the
/// images, their marks hidden and shown as what they mean (`ozero_domain::rich`). `text` is
/// the whole text, changed as it is typed in; `onCaret` says where in it the caret is, for
/// photos to go after its line. Chosen words get a Format menu; a link asks `onLink` for its
/// address with the one it has, if any. A tap on an image opens `imageMenu`.
struct RichBody<ImageMenu: View>: View {
    @Binding var text: String
    let files: [DraftFileItem]
    let images: [Int64: UIImage]
    /// Each new number puts the caret in the text.
    let focusRequest: Int
    let onCaret: (Int) -> Void
    let onFocus: (Bool) -> Void
    let onLink: (String?, @escaping (String?) -> Void) -> Void
    @ViewBuilder let imageMenu: (Int64, ImageSize) -> ImageMenu

    var body: some View {
        let parts = parts
        VStack(alignment: .leading, spacing: 4) {
            ForEach(parts) { part in
                switch part.block {
                case .text(let start, let end):
                    RichTextView(
                        text: (text as NSString).substring(with: NSRange(location: Int(start), length: Int(end - start))),
                        focusRequest: part.id == "t0" ? focusRequest : 0,
                        onChange: { segment, caret in change(part.id, to: segment, caret: caret) },
                        onCaret: { caret in onCaret(Int(start) + caret) },
                        onFocus: onFocus,
                        onLink: onLink
                    )
                    .overlay(alignment: .topLeading) {
                        if text.isEmpty {
                            Text("compose_text").foregroundStyle(.tertiary).allowsHitTesting(false)
                        }
                    }
                case .image(_, _, let file, let size):
                    Menu {
                        imageMenu(file, size)
                    } label: {
                        RichImage(file: files.first { $0.id == file }, image: images[file], size: size)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private struct Part: Identifiable {
        let id: String
        let block: RichBlock
    }

    /// The blocks, each text one named by the image above it so it keeps its caret.
    private var parts: [Part] {
        var above: Int64 = 0
        return richBlocks(text: text).map { block in
            switch block {
            case .text: return Part(id: "t\(above)", block: block)
            case .image(_, _, let file, _):
                above = file
                return Part(id: "i\(file)", block: block)
            }
        }
    }

    /// Where the text block `id` is in the whole text now.
    private func span(of id: String) -> NSRange? {
        for part in parts where part.id == id {
            if case .text(let start, let end) = part.block {
                return NSRange(location: Int(start), length: Int(end - start))
            }
        }
        return nil
    }

    private func change(_ id: String, to segment: String, caret: Int) {
        guard let range = span(of: id) else { return }
        let whole = text as NSString
        if whole.substring(with: range) != segment {
            text = whole.replacingCharacters(in: range, with: segment)
        }
        onCaret(range.location + caret)
    }
}

/// An image in the text, as wide as its size says, with its weight on it.
private struct RichImage: View {
    let file: DraftFileItem?
    let image: UIImage?
    let size: ImageSize

    var body: some View {
        PartOfWidth(part: size == .small ? 0.4 : size == .medium ? 0.66 : 1) {
            Group {
                if let image {
                    Image(uiImage: image).resizable().scaledToFit()
                } else {
                    Rectangle().fill(Color.ozeroCard).aspectRatio(4 / 3, contentMode: .fit)
                }
            }
            .clipShape(.rect(cornerRadius: 10))
            .overlay(alignment: .bottomTrailing) {
                if let file {
                    Text(verbatim: ByteCountFormatter.string(fromByteCount: Int64(clamping: file.size), countStyle: .file))
                        .font(.label)
                        .foregroundStyle(.white)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(.black.opacity(0.55), in: .rect(cornerRadius: 6))
                        .padding(8)
                }
            }
        }
        .padding(.vertical, 4)
        .accessibilityLabel(Text("compose_image"))
    }
}

/// Lays its one view out at a part of the width it gets, from the leading edge.
private struct PartOfWidth: Layout {
    let part: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 320
        let height = subviews.first?.sizeThatFits(ProposedViewSize(width: width * part, height: nil)).height ?? 0
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        subviews.first?.place(at: bounds.origin, proposal: ProposedViewSize(width: bounds.width * part, height: nil))
    }
}

/// One run of text between images in a `UITextView`, whose layout hides the marks.
private struct RichTextView: UIViewRepresentable {
    let text: String
    let focusRequest: Int
    let onChange: (String, Int) -> Void
    let onCaret: (Int) -> Void
    let onFocus: (Bool) -> Void
    let onLink: (String?, @escaping (String?) -> Void) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> UITextView {
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        layout.delegate = context.coordinator
        let container = NSTextContainer(size: CGSize(width: 0, height: CGFloat.greatestFiniteMagnitude))
        container.widthTracksTextView = true
        container.lineFragmentPadding = 0
        storage.addLayoutManager(layout)
        layout.addTextContainer(container)
        let view = UITextView(frame: .zero, textContainer: container)
        view.isScrollEnabled = false
        view.backgroundColor = .clear
        view.textContainerInset = .zero
        view.autocapitalizationType = .sentences
        view.adjustsFontForContentSizeCategory = true
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.delegate = context.coordinator
        context.coordinator.view = view
        context.coordinator.show(text, caret: (text as NSString).length)
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        let coordinator = context.coordinator
        coordinator.parent = self
        if view.text != text {
            coordinator.show(text, caret: min(view.selectedRange.location, (text as NSString).length))
        }
        if focusRequest != coordinator.focusRequest {
            coordinator.focusRequest = focusRequest
            if focusRequest != 0 { DispatchQueue.main.async { view.becomeFirstResponder() } }
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: UITextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 320
        let fits = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
        return CGSize(width: width, height: max(fits.height, Rich.font.lineHeight))
    }

    final class Coordinator: NSObject, UITextViewDelegate, NSLayoutManagerDelegate {
        var parent: RichTextView
        weak var view: UITextView?
        var focusRequest = 0
        /// The characters the layout hides, and the list dashes it shows as bullets.
        private var hidden = IndexSet()
        private var bullets = IndexSet()

        init(_ parent: RichTextView) {
            self.parent = parent
        }

        /// While the text is set from outside, its changes are not told back.
        private var showing = false

        func show(_ text: String, caret: Int) {
            guard let view else { return }
            showing = true
            defer { showing = false }
            view.text = text
            restyle()
            view.selectedRange = NSRange(location: caret, length: 0)
        }

        /// Styles the text by its marks, and has the layout hide them again.
        private func restyle() {
            guard let view else { return }
            let storage = view.textStorage
            let marks = richMarks(text: storage.string)
            hidden = IndexSet()
            bullets = IndexSet()
            let all = NSRange(location: 0, length: storage.length)
            storage.beginEditing()
            storage.setAttributes(Rich.base, range: all)
            for mark in marks {
                if mark.style == .item {
                    bullets.insert(Int(mark.openStart))
                    continue
                }
                hidden.insert(integersIn: Int(mark.openStart)..<Int(mark.openEnd))
                hidden.insert(integersIn: Int(mark.closeStart)..<Int(mark.closeEnd))
                let content = NSRange(location: Int(mark.openEnd), length: Int(mark.closeStart) - Int(mark.openEnd))
                if content.length > 0 { Rich.style(storage, content, mark.style) }
            }
            storage.endEditing()
            // Hiding a mark can change with a character far from it.
            view.layoutManager.invalidateGlyphs(forCharacterRange: all, changeInLength: 0, actualCharacterRange: nil)
            view.layoutManager.invalidateLayout(forCharacterRange: all, actualCharacterRange: nil)
            view.typingAttributes = Rich.base
        }

        private func changed() {
            guard let view else { return }
            parent.onChange(view.text, view.selectedRange.location + view.selectedRange.length)
        }

        func layoutManager(
            _ layoutManager: NSLayoutManager, shouldGenerateGlyphs glyphs: UnsafePointer<CGGlyph>,
            properties: UnsafePointer<NSLayoutManager.GlyphProperty>, characterIndexes: UnsafePointer<Int>,
            font: UIFont, forGlyphRange range: NSRange
        ) -> Int {
            guard !hidden.isEmpty || !bullets.isEmpty else { return 0 }
            var shownGlyphs = Array(UnsafeBufferPointer(start: glyphs, count: range.length))
            var shownProperties = Array(UnsafeBufferPointer(start: properties, count: range.length))
            var bullet: CGGlyph = 0
            if !bullets.isEmpty {
                var dot: UniChar = 0x2022
                CTFontGetGlyphsForCharacters(font as CTFont, &dot, &bullet, 1)
            }
            for i in 0..<range.length {
                let at = characterIndexes[i]
                if hidden.contains(at) {
                    shownProperties[i] = .null
                } else if bullets.contains(at) {
                    shownGlyphs[i] = bullet
                }
            }
            layoutManager.setGlyphs(shownGlyphs, properties: shownProperties, characterIndexes: characterIndexes,
                                    font: font, forGlyphRange: range)
            return range.length
        }

        func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText: String) -> Bool {
            // Letters being composed in a keyboard's own way go as they are.
            guard textView.markedTextRange == nil else { return true }
            let before = textView.text ?? ""
            let after = (before as NSString).replacingCharacters(in: range, with: replacementText)
            // Typing over a mark may need the rest of it to go, or a list to go on.
            guard let fixed = richFix(before: before, after: after) else { return true }
            show(fixed.text, caret: Int(fixed.start))
            changed()
            return false
        }

        func textViewDidChange(_ textView: UITextView) {
            // Restyling would end letters still being composed.
            if textView.markedTextRange == nil { restyle() }
            changed()
        }

        func textViewDidChangeSelection(_ textView: UITextView) {
            // The caret never stops inside hidden marks: a place there counts as after them.
            var range = textView.selectedRange
            if range.length == 0 {
                var at = range.location
                while at < textView.textStorage.length && hidden.contains(at) { at += 1 }
                if at != range.location {
                    range.location = at
                    textView.selectedRange = range
                    return
                }
            }
            if !showing { parent.onCaret(range.location + range.length) }
        }

        func textViewDidBeginEditing(_ textView: UITextView) { parent.onFocus(true) }

        func textViewDidEndEditing(_ textView: UITextView) { parent.onFocus(false) }

        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            guard range.length > 0 else { return nil }
            let styles = Rich.styles.map { style, symbol, key in
                UIAction(title: tr(key), image: UIImage(systemName: symbol)) { [weak self] _ in self?.format(style) }
            }
            let format = UIMenu(title: tr("compose_format"), image: UIImage(systemName: "textformat"), children: styles)
            return UIMenu(children: [format] + suggestedActions)
        }

        private func format(_ style: RichStyle) {
            guard let view else { return }
            guard style == .link else { return apply(style, url: nil) }
            let range = view.selectedRange
            let has = richLinkAt(text: view.text, start: UInt32(range.location), end: UInt32(range.location + range.length))
            parent.onLink(has) { [weak self] url in self?.apply(.link, url: url, in: range) }
        }

        private func apply(_ style: RichStyle, url: String?, in chosen: NSRange? = nil) {
            guard let view else { return }
            let range = chosen ?? view.selectedRange
            let done = richToggle(text: view.text, start: UInt32(range.location), end: UInt32(range.location + range.length),
                                  style: style, url: url)
            show(done.text, caret: Int(done.start))
            view.selectedRange = NSRange(location: Int(done.start), length: Int(done.end) - Int(done.start))
            changed()
        }
    }
}

/// How the editor draws the styles.
private enum Rich {
    static var font: UIFont { .preferredFont(forTextStyle: .body) }
    static var base: [NSAttributedString.Key: Any] { [.font: font, .foregroundColor: UIColor.label] }

    /// Each style of the Format menu, with its symbol and name.
    static let styles: [(RichStyle, String, String)] = [
        (.bold, "bold", "compose_format_bold"),
        (.italic, "italic", "compose_format_italic"),
        (.strike, "strikethrough", "compose_format_strike"),
        (.code, "chevron.left.forwardslash.chevron.right", "compose_format_code"),
        (.link, "link", "compose_format_link"),
        (.heading, "textformat.size.larger", "compose_format_heading"),
        (.item, "list.bullet", "compose_format_list"),
        (.quote, "text.quote", "compose_format_quote"),
    ]

    static func style(_ storage: NSTextStorage, _ range: NSRange, _ style: RichStyle) {
        switch style {
        case .bold: add(.traitBold, storage, range)
        case .italic: add(.traitItalic, storage, range)
        case .strike: storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
        case .code:
            storage.addAttribute(.font, value: UIFont.monospacedSystemFont(ofSize: font.pointSize * 0.94, weight: .regular), range: range)
            storage.addAttribute(.backgroundColor, value: UIColor.secondarySystemFill, range: range)
        case .link:
            storage.addAttribute(.foregroundColor, value: UIColor(Color.ozeroFill), range: range)
            storage.addAttribute(.underlineStyle, value: NSUnderlineStyle.single.rawValue, range: range)
        case .heading:
            storage.addAttribute(.font, value: UIFont.preferredFont(forTextStyle: .title3), range: range)
            add(.traitBold, storage, range)
        case .quote:
            storage.addAttribute(.foregroundColor, value: UIColor.secondaryLabel, range: range)
            add(.traitItalic, storage, range)
        case .item, .escape: break
        }
    }

    /// Adds `trait` to the fonts the range has, so styles inside one another add up.
    private static func add(_ trait: UIFontDescriptor.SymbolicTraits, _ storage: NSTextStorage, _ range: NSRange) {
        storage.enumerateAttribute(.font, in: range) { value, part, _ in
            let font = value as? UIFont ?? Rich.font
            let traits = font.fontDescriptor.symbolicTraits.union(trait)
            guard let descriptor = font.fontDescriptor.withSymbolicTraits(traits) else { return }
            storage.addAttribute(.font, value: UIFont(descriptor: descriptor, size: font.pointSize), range: part)
        }
    }
}

/// The images of a draft's text, read from the phone and made small enough to show.
func shownImage(_ data: Data) -> UIImage? {
    guard let image = UIImage(data: data) else { return nil }
    let width = 1200.0
    guard image.size.width > width else { return image }
    return image.preparingThumbnail(of: CGSize(width: width, height: image.size.height * width / image.size.width))
}
