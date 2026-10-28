import Contacts
import ContactsUI
import SwiftUI

/// The phone's contacts: read for "To" and names, shown and added to in the system's own card.
enum PhoneContacts {
    /// The person let the app read them, all or the ones they chose.
    static var allowed: Bool {
        switch CNContactStore.authorizationStatus(for: .contacts) {
        case .authorized, .limited: true
        default: false
        }
    }

    /// The phone has not asked yet: "To" offers them once.
    static var askable: Bool { CNContactStore.authorizationStatus(for: .contacts) == .notDetermined }

    /// Asks the phone for them; true once they may be read.
    static func ask() async -> Bool {
        (try? await CNContactStore().requestAccess(for: .contacts)) ?? false
    }

    /// Every contact with an address, by its identifier; none without access. Reads the whole
    /// address book, so never on the main thread.
    static func read() -> [ContactItem] {
        guard allowed else { return [] }
        let keys: [CNKeyDescriptor] = [
            CNContactFormatter.descriptorForRequiredKeys(for: .fullName),
            CNContactEmailAddressesKey as CNKeyDescriptor,
        ]
        var found: [ContactItem] = []
        try? CNContactStore().enumerateContacts(with: CNContactFetchRequest(keysToFetch: keys)) { contact, _ in
            var emails: [String] = []
            for value in contact.emailAddresses {
                let email = (value.value as String).trimmingCharacters(in: .whitespaces)
                if !email.isEmpty && !emails.contains(where: { $0.caseInsensitiveCompare(email) == .orderedSame }) {
                    emails.append(email)
                }
            }
            let name = CNContactFormatter.string(from: contact, style: .fullName) ?? ""
            if !emails.isEmpty { found.append(ContactItem(key: contact.identifier, name: name, emails: emails)) }
        }
        return found
    }
}

/// The system's card of a contact: the one with `key`, or a new one with `name` and `email`.
struct ContactCard: UIViewControllerRepresentable {
    enum Shown: Identifiable {
        case contact(key: String)
        case new(name: String, email: String)

        var id: String {
            switch self {
            case .contact(let key): "c" + key
            case .new(_, let email): "n" + email
            }
        }
    }

    let shown: Shown
    let onClose: () -> Void

    func makeUIViewController(context: Context) -> UINavigationController {
        let card: CNContactViewController
        switch shown {
        case .contact(let key):
            let keys = [CNContactViewController.descriptorForRequiredKeys()]
            if let contact = try? CNContactStore().unifiedContact(withIdentifier: key, keysToFetch: keys) {
                card = CNContactViewController(for: contact)
            } else {
                card = CNContactViewController(forNewContact: nil)
            }
        case .new(let name, let email):
            let contact = CNMutableContact()
            if name != email { contact.givenName = name }
            contact.emailAddresses = [CNLabeledValue(label: CNLabelHome, value: email as NSString)]
            card = CNContactViewController(forNewContact: contact)
        }
        card.delegate = context.coordinator
        card.navigationItem.leftBarButtonItem = UIBarButtonItem(
            systemItem: .close, primaryAction: UIAction { _ in onClose() })
        return UINavigationController(rootViewController: card)
    }

    func updateUIViewController(_ controller: UINavigationController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onClose: onClose) }

    final class Coordinator: NSObject, CNContactViewControllerDelegate {
        let onClose: () -> Void

        init(onClose: @escaping () -> Void) {
            self.onClose = onClose
        }

        func contactViewController(_ controller: CNContactViewController, didCompleteWith contact: CNContact?) {
            onClose()
        }
    }
}
