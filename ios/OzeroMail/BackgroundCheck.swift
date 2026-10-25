import BackgroundTasks
import UserNotifications
import os

/// Checks every account for new mail while the app is closed and tells about mail from people.
/// iOS decides when it runs; asking for every 15 minutes is a wish, not a schedule.
enum BackgroundCheck {
    static let id = "com.ozeromail.app.refresh"

    /// Asks for the next check; a newer request replaces the waiting one.
    static func schedule() {
        let request = BGAppRefreshTaskRequest(identifier: id)
        request.earliestBeginDate = .now.addingTimeInterval(15 * 60)
        do {
            try BGTaskScheduler.shared.submit(request)
        } catch {
            log.error("background check not scheduled: \(String(describing: error), privacy: .public)")
        }
    }

    static func run() async {
        schedule()
        do {
            let mail = try await Mail.shared()
            // Other accounts may still have synced; the next run tries again.
            _ = try? await mail.refresh()
            let new = try await mail.newMail()
            await notify(new)
        } catch {
            log.error("background check failed: \(String(describing: error), privacy: .public)")
        }
    }

    /// More at once is noise; the newest are shown.
    private static let maxShown = 5

    /// One notification per message, grouped in one thread, and one more line when there are
    /// more than shown; a tap opens the app.
    private static func notify(_ messages: [MessageItem]) async {
        let center = UNUserNotificationCenter.current()
        if messages.count > maxShown {
            let content = UNMutableNotificationContent()
            content.body = String(localized: "notify_more")
            content.threadIdentifier = "new_mail"
            try? await center.add(UNNotificationRequest(identifier: "more", content: content, trigger: nil))
        }
        for message in messages.prefix(maxShown) {
            let content = UNMutableNotificationContent()
            content.title = message.sender
            content.subtitle =
                message.subject.isEmpty ? String(localized: "inbox_no_subject") : message.subject
            content.body = message.snippet
            content.sound = .default
            content.threadIdentifier = "new_mail"
            let request = UNNotificationRequest(
                identifier: "message-\(message.id)", content: content, trigger: nil)
            // Without permission this fails quietly, which is what the person chose.
            try? await center.add(request)
        }
    }
}

private let log = Logger(subsystem: "com.ozeromail.app", category: "background")
