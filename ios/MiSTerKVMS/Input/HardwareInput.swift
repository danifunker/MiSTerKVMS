import GameController
import UIKit

/// A hardware keyboard, mouse or trackpad (an iPad's Magic Keyboard, or
/// Bluetooth/USB ones on an iPad or iPhone) driving the MiSTer directly.
///
/// - Keys arrive as presses on the key input view (`KeyInputView`), with
///   their USB HID usage, and go down and up on the MiSTer as they do here.
///   This only says whether a keyboard is connected, so that view keeps the
///   focus even with the on-screen keyboard and the function bar hidden.
/// - The mouse is read raw (GameController): movement and buttons go to the
///   MiSTer while the pointer is over the picture (`pointerOverPicture`),
///   where the iPad's own pointer is hidden; over the app's buttons the
///   pointer is shown and works as usual.
@MainActor
final class HardwareInput: ObservableObject {
    @Published private(set) var keyboardConnected = GCKeyboard.coalesced != nil
    weak var model: AppModel?
    /// Off while a sheet or Arrange mode is up.
    var enabled = true { didSet { if !enabled { releaseButtons() } } }
    var pointerOverPicture = false { didSet { if !pointerOverPicture { releaseButtons() } } }
    private var held: Set<MouseButton> = []
    private var observers: [NSObjectProtocol] = []

    init() {
        let center = NotificationCenter.default
        for name in [Notification.Name.GCKeyboardDidConnect, .GCKeyboardDidDisconnect] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.keyboardConnected = GCKeyboard.coalesced != nil }
            })
        }
        observers.append(center.addObserver(forName: .GCMouseDidConnect, object: nil, queue: .main) { [weak self] note in
            guard let mouse = note.object as? GCMouse else { return }
            MainActor.assumeIsolated { self?.attach(mouse) }
        })
        GCMouse.mice().forEach(attach)
    }

    deinit { observers.forEach(NotificationCenter.default.removeObserver) }

    private var forwarding: Bool { enabled && pointerOverPicture }

    private func attach(_ mouse: GCMouse) {
        guard let input = mouse.mouseInput else { return }
        // Handlers run on the main queue (GameController's default).
        input.mouseMovedHandler = { [weak self] _, dx, dy in
            MainActor.assumeIsolated {
                guard let self, self.forwarding else { return }
                // GameController's y grows upwards; the MiSTer's downwards.
                self.model?.move(dx: Double(dx), dy: Double(-dy))
            }
        }
        let buttons: [(GCControllerButtonInput?, MouseButton)] =
            [(input.leftButton, .left), (input.rightButton, .right), (input.middleButton, .middle)]
        for (control, button) in buttons {
            control?.pressedChangedHandler = { [weak self] _, _, pressed in
                MainActor.assumeIsolated { self?.press(button, pressed) }
            }
        }
    }

    private func press(_ button: MouseButton, _ down: Bool) {
        if down {
            guard forwarding, held.insert(button).inserted else { return }
        } else {
            guard held.remove(button) != nil else { return }
        }
        model?.button(button, down: down)
    }

    /// A button held when the pointer leaves the picture is let go there.
    private func releaseButtons() {
        for button in held { model?.button(button, down: false) }
        held = []
    }
}
