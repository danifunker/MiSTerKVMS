import AVFoundation
import UIKit
import UniformTypeIdentifiers

/// UI-facing state for the app: the saved hosts and the one open session.
/// The session logic lives in the core's Controller (as on the Mac); this
/// mirrors its state for SwiftUI and adds what's iOS-specific: the video
/// view, touch input, the on-screen modifiers and the app lifecycle.
@MainActor
final class AppModel: ObservableObject {
    let core: Controller
    @Published private(set) var state: ControllerState
    @Published private(set) var profiles: [HostProfile] = []
    /// Function bar modifiers (HID usages): latched ones apply to the next
    /// key only, locked ones until tapped again.
    @Published private(set) var latched: Set<UInt16> = []
    @Published private(set) var locked: Set<UInt16> = []

    private weak var videoView: UIView?
    /// The host that was open when the app went to the background.
    private var suspendedId: String?

    init() {
        let observer = Observer()
        core = Controller(observer: observer)
        // A touch screen captures as soon as a host connects (D6), whatever
        // the desktop apps' Capture automatically setting says.
        core.forceAutoCapture()
        state = core.state()
        observer.model = self
        profiles = core.profiles()
        loadKeys()
        showWelcome = !UserDefaults.standard.bool(forKey: Pref.welcomed)
        if showWelcome && profiles.contains(where: { !$0.isDemo }) {
            // Set up before the first-run screen existed.
            UserDefaults.standard.set(true, forKey: Pref.welcomed)
            showWelcome = false
        }
        syncDemoHost()
        // Other viewers see this name ("iPhone has control"). The core falls
        // back to "MiSTerKVMS" where it can't read a computer name, as on iOS.
        if core.viewerName() == "MiSTerKVMS" {
            core.setViewerName(name: UIDevice.current.name)
        }
        let audio = AVAudioSession.sharedInstance()
        try? audio.setCategory(.playback, mode: .default, options: [.mixWithOthers])
        try? audio.setActive(true)
    }

    fileprivate func handle(_ event: ControllerEvent) {
        switch event {
        case .changed:
            state = core.state()
        case .profilesChanged:
            profiles = core.profiles()
            state = core.state()
            saveMovedKeys()
        case .captureRequested:
            // A touch screen is always "capturing" while connected (D6).
            if !someoneElseInControl && !watching { core.setCapturing(capturing: true) }
        case .releaseRequested:
            core.setCapturing(capturing: false)
        case .previewsChanged:
            break
        }
    }

    // MARK: - State

    var profile: HostProfile? {
        guard let id = state.profileId else { return nil }
        return profiles.first { $0.id == id }
    }

    var isConnected: Bool {
        if case .connected = state.connection { return true }
        return false
    }

    /// Control given up from here: no input is sent (it would take control
    /// back) until the user takes control again.
    @Published private(set) var watching = false

    /// This phone's input reaches the MiSTer: connected to a capture server
    /// stream, not watching, nobody else in control.
    var inControl: Bool { state.stream != nil && !watching && !someoneElseInControl }

    /// A capture server stream that someone else controls.
    var someoneElseInControl: Bool {
        guard let stream = state.stream else { return false }
        return !stream.haveControl && !stream.holder.isEmpty
    }

    var muted: Bool {
        get { state.muted }
        set { core.setMuted(muted: newValue) }
    }

    // MARK: - Hosts

    func save(_ profile: HostProfile) { core.saveProfile(profile: profile) }

    func delete(_ id: String) {
        let server = core.deviceKeyServer(id: id)
        core.deleteProfile(id: id)
        profiles = core.profiles()
        if !profiles.contains(where: { $0.useInternet && core.deviceKeyServer(id: $0.id) == server }) {
            DeviceKeychain.delete(server)
        }
    }

    // MARK: - Demo (docs/APP-STORE.md S1)

    /// The first-run screen: try the demo, or go on to the MiSTers.
    @Published private(set) var showWelcome = false

    /// The Demo host is in the list: the test pattern, with the input it
    /// receives listed on screen instead of sent to a MiSTer.
    var demoMode: Bool {
        get { UserDefaults.standard.bool(forKey: Pref.demoMode) }
        set {
            UserDefaults.standard.set(newValue, forKey: Pref.demoMode)
            if !newValue, profile?.isDemo == true { close() }
            syncDemoHost()
            objectWillChange.send()
        }
    }

    var isDemo: Bool { profile?.isDemo == true }

    private var openDemoWhenListed = false

    /// The host list is showing: open the demo if the first-run screen asked.
    func listShown() {
        guard openDemoWhenListed else { return }
        openDemoWhenListed = false
        if let demo = profiles.first(where: \.isDemo) { open(demo) }
    }

    /// The first-run screen's answer: `demo` turns demo mode on and opens it.
    func welcomed(demo: Bool) {
        UserDefaults.standard.set(true, forKey: Pref.welcomed)
        showWelcome = false
        if demo {
            demoMode = true
            // Opened by the host list once it has replaced this screen: the
            // session's full-screen cover doesn't come up during that swap.
            openDemoWhenListed = true
        }
    }

    /// The Demo host saved while demo mode is on, gone while it's off.
    private func syncDemoHost() {
        let have = profiles.contains(where: \.isDemo)
        if demoMode && !have {
            core.saveProfile(profile: demoHostProfile())
        } else if !demoMode && have {
            core.deleteProfile(id: demoHostProfile().id)
        }
        profiles = core.profiles()
    }

    // MARK: - This iPhone's name

    /// Shown on the capture PC's list of paired devices and to other viewers
    /// ("Dani's iPhone has control"). iOS only tells apps "iPhone", so the
    /// person names it.
    var deviceName: String {
        get { core.viewerName() }
        set {
            let name = newValue.trimmingCharacters(in: .whitespaces)
            core.setViewerName(name: name.isEmpty ? UIDevice.current.name : name)
            objectWillChange.send()
        }
    }

    /// Still the generic name: ask for a real one before pairing.
    var deviceNameIsDefault: Bool { deviceName == UIDevice.current.name || deviceName == Device.kind }

    // MARK: - Over the internet (docs/IPHONE.md §2–3)

    /// This phone holds a key for the MiSTer's capture PC.
    func isPaired(_ profile: HostProfile) -> Bool { core.hasDeviceKey(id: profile.id) }

    /// Use the MiSTer over the internet: reuses this phone's key for its
    /// capture PC, or pairs (on the home network) and keeps the new key in
    /// the Keychain. `again`: a new key, e.g. after the capture PC revoked
    /// the old one. Throws the server's own explanation, e.g. internet
    /// access turned off on the capture PC.
    func pairForInternet(_ profile: HostProfile, again: Bool = false) async throws {
        let core = self.core
        let id = profile.id
        let name = deviceName
        let result = try await Task.detached { try core.pairDevice(id: id, deviceName: name, again: again) }.value
        let have = DeviceKeychain.load(result.serverId)
        if !result.reused || have == nil {
            guard DeviceKeychain.save(.init(keyId: result.keyId, key: result.key), as: result.serverId) else {
                // Without the key saved, the next launch couldn't connect from outside.
                _ = core.stopInternet(id: id)
                profiles = core.profiles()
                throw ControllerError.Failed(message: "this \(Device.kind) couldn't save the key in its Keychain")
            }
        }
        profiles = core.profiles()
    }

    /// Stop connecting this MiSTer from the internet. The key goes too when
    /// no other MiSTer of the same capture PC uses it. (The capture PC's
    /// setup page can revoke it there.)
    func stopInternet(_ profile: HostProfile) {
        let server = core.deviceKeyServer(id: profile.id)
        if !core.stopInternet(id: profile.id) {
            DeviceKeychain.delete(server)
        }
        profiles = core.profiles()
    }

    /// Keys from the Keychain, one per capture PC (the core never stores them).
    private func loadKeys() {
        for profile in profiles where profile.useInternet {
            let server = core.deviceKeyServer(id: profile.id)
            // Builds before one key per capture PC filed it under the MiSTer.
            if DeviceKeychain.load(server) == nil, let old = DeviceKeychain.load(profile.id) {
                DeviceKeychain.save(old, as: server)
            }
            DeviceKeychain.delete(profile.id)
            if let entry = DeviceKeychain.load(server) {
                core.setDeviceKey(serverId: server, keyId: entry.keyId, key: entry.key)
            }
        }
    }

    /// A key kept by a capture PC's address moves to its id once the core
    /// learns the id: keep the Keychain in step.
    private func saveMovedKeys() {
        for profile in profiles where profile.useInternet {
            let server = core.deviceKeyServer(id: profile.id)
            if DeviceKeychain.load(server) == nil, let key = core.deviceKey(serverId: server) {
                DeviceKeychain.save(.init(keyId: key.keyId, key: key.key), as: server)
            }
        }
    }

    // MARK: - Predictive cursor

    /// For the route in use: the core keeps home's and the internet's apart.
    var predictiveCursor: Bool {
        get { state.display.predictiveCursor }
        set {
            var display = state.display
            display.predictiveCursor = newValue
            core.setDisplay(settings: display)
        }
    }

    func open(_ profile: HostProfile) {
        guard profile.id != state.profileId else { return }
        latched = []
        watching = false
        core.open(id: profile.id)
        state = core.state()
    }

    func close() {
        core.releaseAll()
        core.close()
        state = core.state()
    }

    /// The next (+1) or previous (-1) host, wrapping around.
    func step(_ offset: Int) {
        guard profiles.count > 1, let id = state.profileId,
              let i = profiles.firstIndex(where: { $0.id == id }) else { return }
        switchTo(profiles[(i + offset + profiles.count) % profiles.count])
    }

    /// Leave the open MiSTer (keys and buttons released) for another one.
    func switchTo(_ profile: HostProfile) {
        guard profile.id != state.profileId else { return }
        core.releaseAll()
        core.close()
        open(profile)
    }

    /// iOS suspends sockets in the background: let go of the MiSTer cleanly
    /// and reconnect when the app is back.
    func suspend() {
        guard let id = state.profileId else { return }
        suspendedId = id
        close()
    }

    func resume() {
        guard let id = suspendedId else { return }
        suspendedId = nil
        if let profile = profiles.first(where: { $0.id == id }) { open(profile) }
    }

    // MARK: - Video view

    func attach(view: UIView) {
        guard view !== videoView else { return }
        if videoView != nil { core.detachSurface() }
        videoView = view
        let size = Self.pixelSize(of: view)
        core.attachSurface(
            surface: .uiKit(uiView: UInt64(UInt(bitPattern: Unmanaged.passUnretained(view).toOpaque()))),
            widthPx: size.width, heightPx: size.height, pointsScale: size.scale)
        askForScreenSize(size)
    }

    func detach(view: UIView) {
        guard view === videoView else { return }
        core.detachSurface()
        videoView = nil
    }

    func resized(view: UIView) {
        guard view === videoView else { return }
        let size = Self.pixelSize(of: view)
        core.resizeSurface(widthPx: size.width, heightPx: size.height, pointsScale: size.scale)
        askForScreenSize(size)
    }

    /// No bigger picture than the phone shows: the server scales it down
    /// (1080p → 720p in portrait), a third of the data.
    private func askForScreenSize(_ size: (width: UInt32, height: UInt32, scale: Float)) {
        core.setStreamLimits(limits: StreamLimits(maxWidth: size.width, maxHeight: size.height, maxFps: nil, quality: nil))
    }

    private static func pixelSize(of view: UIView) -> (width: UInt32, height: UInt32, scale: Float) {
        let scale = view.window?.screen.scale ?? view.contentScaleFactor
        return (UInt32(max(1, view.bounds.width * scale)), UInt32(max(1, view.bounds.height * scale)), Float(scale))
    }

    // MARK: - Input

    /// Input from a viewer without control is dropped by the server; the
    /// first input on a free stream takes control (CAPTURE-SERVER.md §7).
    private func ready() -> Bool {
        guard isConnected, !someoneElseInControl, !watching else { return false }
        if !state.capturing { core.setCapturing(capturing: true) }
        return true
    }

    func takeControl() {
        watching = false
        core.takeControl()
        core.setCapturing(capturing: true)
    }

    /// Let another viewer have the MiSTer; this phone only watches.
    func releaseControl() {
        watching = true
        core.releaseAll()
        core.releaseControl()
        core.setCapturing(capturing: false)
    }

    func openMenu() {
        guard ready() else { return }
        core.openMenu()
    }

    /// Text from the system keyboard.
    func type(_ text: String) {
        guard ready() else { return }
        core.typeText(text: text, modifiers: takeModifiers())
    }

    /// A key from the function bar or the remote, with the modifiers held.
    func tap(_ usage: UInt16) {
        guard ready() else { return }
        let modifiers = takeModifiers()
        if modifiers.isEmpty {
            core.tapKey(hidUsage: usage)
        } else {
            core.pressCombo(hidUsages: modifiers + [usage])
        }
    }

    /// A hardware keyboard's key went down or up (USB HID usage); held keys
    /// stay held on the MiSTer. A press is held at least `minimumHold`: the
    /// MiSTer misses a key let go the moment it went down.
    func key(_ usage: UInt16, down: Bool) {
        guard ready() else { return }
        if down {
            keyDownAt[usage] = Date()
            _ = core.keyEvent(hidUsage: usage, down: true)
            return
        }
        let held = keyDownAt.removeValue(forKey: usage).map { Date().timeIntervalSince($0) } ?? Self.minimumHold
        if held >= Self.minimumHold {
            _ = core.keyEvent(hidUsage: usage, down: false)
        } else {
            DispatchQueue.main.asyncAfter(deadline: .now() + Self.minimumHold - held) { [weak self] in
                guard let self, self.keyDownAt[usage] == nil else { return }  // pressed again meanwhile
                _ = self.core.keyEvent(hidUsage: usage, down: false)
            }
        }
    }

    private static let minimumHold: TimeInterval = 0.04
    private var keyDownAt: [UInt16: Date] = [:]

    /// Lets go of every key and button held on the MiSTer.
    func releaseKeys() {
        keyDownAt = [:]
        guard isConnected else { return }
        core.releaseAll()
    }

    /// The Type Text box: `text` typed key by key at `keysPerSec`
    /// (remembered for the host by the core).
    func typeText(_ text: String, keysPerSec: Double) {
        guard ready() else { return }
        core.typeTextAt(text: text, keysPerSec: keysPerSec)
    }

    func move(dx: Double, dy: Double) {
        guard ready() else { return }
        core.mouseMove(dx: dx, dy: dy)
    }

    func button(_ button: MouseButton, down: Bool) {
        guard ready() else { return }
        core.mouseButton(button: button, down: down)
    }

    func click(_ button: MouseButton) {
        guard ready() else { return }
        core.click(button: button)
    }

    func zoom(by factor: Double) { core.zoomBy(factor: factor) }
    func resetZoom() { core.resetZoom() }

    // MARK: - Screenshots

    /// "Screenshot copied" (or why not) over the picture for a moment.
    @Published private(set) var screenshotNotice: String?
    private var noticeGeneration = 0

    /// Encryption with capture PCs at home: "auto", "require" or "off".
    var homeEncryption: String {
        get { core.homeEncryption() }
        set {
            core.setHomeEncryption(mode: newValue)
            objectWillChange.send()
        }
    }

    /// Screenshots get a bar with the core and the time.
    var screenshotStamp: Bool {
        get { core.screenshotStamp() }
        set {
            core.setScreenshotStamp(on: newValue)
            objectWillChange.send()
        }
    }

    /// The picture as it is now, onto the clipboard as a PNG.
    func copyScreenshot() {
        let format = DateFormatter()
        format.locale = Locale(identifier: "en_US_POSIX")
        format.dateFormat = "yyyy-MM-dd HH:mm:ss"
        let notice: String
        do {
            let shot = try core.screenshot(takenAt: format.string(from: Date()))
            UIPasteboard.general.setData(shot.png, forPasteboardType: UTType.png.identifier)
            notice = "Screenshot copied"
        } catch ControllerError.Failed(let message) {
            notice = "No screenshot: \(message)"
        } catch {
            notice = "No screenshot: \(error.localizedDescription)"
        }
        noticeGeneration += 1
        let generation = noticeGeneration
        screenshotNotice = notice
        DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in
            guard let self, self.noticeGeneration == generation else { return }
            self.screenshotNotice = nil
        }
    }
    /// Pan by a fraction of the picture.
    func pan(dx: Double, dy: Double) { core.panBy(dx: dx, dy: dy) }

    // MARK: - Function bar modifiers

    enum ModifierState { case off, latched, locked }

    func modifierState(_ usage: UInt16) -> ModifierState {
        locked.contains(usage) ? .locked : latched.contains(usage) ? .latched : .off
    }

    /// Tap cycles off → next key only → locked → off.
    func cycleModifier(_ usage: UInt16) {
        switch modifierState(usage) {
        case .off: latched.insert(usage)
        case .latched: latched.remove(usage); locked.insert(usage)
        case .locked: locked.remove(usage)
        }
    }

    private func takeModifiers() -> [UInt16] {
        let all = latched.union(locked).sorted()
        latched = []
        return all
    }
}

/// Bridges core events (background threads) onto the main actor.
private final class Observer: ControllerObserver, @unchecked Sendable {
    weak var model: AppModel?

    func onEvent(event: ControllerEvent) {
        DispatchQueue.main.async { [weak self] in
            MainActor.assumeIsolated { self?.model?.handle(event) }
        }
    }
}
