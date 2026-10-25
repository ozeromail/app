import UIKit

/// Time iOS gives work that must finish after the app leaves the screen, such as an action on
/// its way to the server.
@MainActor
final class Background {
    private var id = UIBackgroundTaskIdentifier.invalid

    private init() {}

    static func begin(_ name: String) -> Background {
        let task = Background()
        task.id = UIApplication.shared.beginBackgroundTask(withName: name) {
            // Out of time: what is left stays queued for the next push or sync.
            task.end()
        }
        return task
    }

    /// Once, whether the work finished or the time ran out.
    func end() {
        guard id != .invalid else { return }
        UIApplication.shared.endBackgroundTask(id)
        id = .invalid
    }
}
