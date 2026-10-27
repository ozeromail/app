import MetricKit
import UIKit

/// Where letters to the author go: the same address as on the site.
private let author = "khizrim@khizrim.ru"

/// A letter to the author, with what the app runs on below, and a crash report if there is one.
@MainActor
func authorDraft(crash: String? = nil) -> DraftItem {
    let device = "Ozero Mail \(appVersion), iOS \(UIDevice.current.systemVersion), \(Crashes.model)"
    let text = ["\n\n-- ", device, crash].compactMap { $0 }.joined(separator: "\n")
    var draft = newDraft
    draft.to = author
    draft.subject = tr(crash == nil ? "feedback_subject" : "crash_subject")
    // Plain text: a crash's marks stay as they are.
    draft.text = richEscape(plain: text)
    return draft
}

/// The app's version, as the store shows it.
let appVersion = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"

/// The last crash, kept on the phone until the person sends it or lets it go. Nothing leaves the
/// phone on its own: the report becomes a letter the person reads before sending. iOS hands the
/// app its crashes on a later launch.
final class Crashes: NSObject, MXMetricManagerSubscriber {
    static let shared = Crashes()
    private static let limit = 16 * 1024

    func install() {
        MXMetricManager.shared.add(self)
    }

    /// Where the code failed, without the reason's text: it can hold a server's reply, an
    /// address or a subject.
    func didReceive(_ payloads: [MXDiagnosticPayload]) {
        guard let crash = payloads.compactMap(\.crashDiagnostics).joined().last else { return }
        let stack = String(decoding: crash.callStackTree.jsonRepresentation(), as: UTF8.self)
        let kind = [crash.exceptionType.map { "exception \($0)" }, crash.signal.map { "signal \($0)" }]
            .compactMap { $0 }.joined(separator: ", ")
        let report = "\(kind)\n\(stack)"
        try? String(report.prefix(Self.limit)).write(to: Self.file, atomically: true, encoding: .utf8)
    }

    static func last() -> String? {
        try? String(contentsOf: file, encoding: .utf8)
    }

    static func forget() {
        try? FileManager.default.removeItem(at: file)
    }

    private static var file: URL {
        URL.applicationSupportDirectory.appending(path: "crash.txt")
    }

    /// The phone's model, as "iPhone15,2".
    static var model: String {
        var info = utsname()
        uname(&info)
        return withUnsafeBytes(of: &info.machine) { String(decoding: $0.prefix { $0 != 0 }, as: UTF8.self) }
    }
}
