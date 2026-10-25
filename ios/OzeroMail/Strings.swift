import Foundation

/// The string for `key` in the app's language, with `args` in its placeholders.
/// For keys only known at run time, or with arguments; `Text("key")` does the rest.
func tr(_ key: String, _ args: CVarArg...) -> String {
    let format = Bundle.main.localizedString(forKey: key, value: nil, table: nil)
    return args.isEmpty ? format : String(format: format, locale: .current, arguments: args)
}

extension String {
    /// Nothing but spaces and line breaks.
    var isBlank: Bool { trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
}
