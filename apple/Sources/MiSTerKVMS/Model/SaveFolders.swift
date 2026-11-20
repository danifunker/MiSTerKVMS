import AppKit
import Foundation

/// Where the App Store build saves screenshots and recordings.
///
/// The sandboxed app may only write where the user points it. The user picks
/// a folder in the standard Open panel (in Settings, or with "Use this folder
/// from now on" when saving), and we keep a security-scoped bookmark to it
/// (entitlement files.bookmarks.app-scope), so later saves need no panel.
/// No folder, or one that's gone: the panel asks again.
@MainActor
final class SaveFolders: ObservableObject {
    static let shared = SaveFolders()

    enum Kind: String, CaseIterable {
        case screenshots, recordings

        var title: String { self == .screenshots ? "Screenshots" : "Recordings" }

        /// Where the panel starts the first time.
        var startDirectory: URL? {
            FileManager.default.urls(for: self == .screenshots ? .picturesDirectory : .moviesDirectory,
                                     in: .userDomainMask).first
        }

        fileprivate var defaultsKey: String { "saveFolder.\(rawValue)" }
    }

    /// The folders in use, resolved and open for writing, by kind.
    @Published private(set) var folders: [Kind: URL] = [:]
    /// Folders the user picked that can't be found now (moved, deleted, a
    /// drive unplugged), so the panel can say why it's asking.
    @Published private(set) var missing: Set<Kind> = []

    private init() {
        for kind in Kind.allCases { resolve(kind) }
    }

    /// The folder to save into without asking, if the user set one and it's
    /// still there.
    func folder(_ kind: Kind) -> URL? {
        if let url = folders[kind], Self.isFolder(url) { return url }
        // Gone since launch, or back since (a drive plugged in again).
        resolve(kind)
        return folders[kind]
    }

    /// Remember `url` (picked in an Open panel) for `kind`.
    func set(_ kind: Kind, to url: URL) {
        do {
            let data = try url.bookmarkData(options: .withSecurityScope,
                                            includingResourceValuesForKeys: nil, relativeTo: nil)
            UserDefaults.standard.set(data, forKey: kind.defaultsKey)
            open(kind, url)
        } catch {
            // Not remembered: saving still works this once, through the panel.
            NSLog("save folder: couldn't remember \(url.path): \(error)")
        }
    }

    /// Back to asking each time.
    func reset(_ kind: Kind) {
        UserDefaults.standard.removeObject(forKey: kind.defaultsKey)
        close(kind)
        missing.remove(kind)
    }

    /// Settings' Choose… button: the Open panel, then remember the folder.
    func choose(_ kind: Kind) {
        let panel = Self.folderPanel(for: kind, current: folders[kind])
        panel.title = "Choose a Folder for \(kind.title)"
        panel.message = "MiSTerKVMS saves \(kind.rawValue) here without asking."
        panel.prompt = "Use This Folder"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        set(kind, to: url)
    }

    /// When nothing is set: the Open panel to pick a folder for this one save,
    /// with "Use this folder from now on". Nil if cancelled.
    func ask(_ kind: Kind, title: String, prompt: String) -> URL? {
        let panel = Self.folderPanel(for: kind, current: nil)
        panel.title = title
        panel.prompt = prompt
        panel.message = missing.contains(kind)
            ? "The folder you chose for \(kind.rawValue) can't be found. Choose where to save."
            : "Choose where to save."
        let remember = NSButton(checkboxWithTitle: "Use this folder for \(kind.rawValue) from now on",
                                target: nil, action: nil)
        remember.state = .off
        let accessory = NSView(frame: NSRect(x: 0, y: 0, width: remember.fittingSize.width + 40,
                                             height: remember.fittingSize.height + 16))
        remember.frame.origin = NSPoint(x: 20, y: 8)
        accessory.addSubview(remember)
        panel.accessoryView = accessory
        panel.isAccessoryViewDisclosed = true
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        if remember.state == .on { set(kind, to: url) }
        return url
    }

    /// The folder's name for Settings, or nil when asking each time.
    func displayName(_ kind: Kind) -> String? {
        folders[kind].map { FileManager.default.displayName(atPath: $0.path) }
    }

    // MARK: - Bookmarks

    private func resolve(_ kind: Kind) {
        guard let data = UserDefaults.standard.data(forKey: kind.defaultsKey) else {
            close(kind)
            missing.remove(kind)
            return
        }
        var stale = false
        guard let url = try? URL(resolvingBookmarkData: data, options: .withSecurityScope,
                                 relativeTo: nil, bookmarkDataIsStale: &stale),
              open(kind, url), Self.isFolder(url) else {
            close(kind)
            missing.insert(kind)
            return
        }
        missing.remove(kind)
        if stale {
            // Moved or renamed: store a fresh bookmark while we have access.
            if let fresh = try? url.bookmarkData(options: .withSecurityScope,
                                                 includingResourceValuesForKeys: nil, relativeTo: nil) {
                UserDefaults.standard.set(fresh, forKey: kind.defaultsKey)
            }
        }
    }

    /// Start access (kept until the app quits or the folder changes).
    @discardableResult
    private func open(_ kind: Kind, _ url: URL) -> Bool {
        if folders[kind] == url { return true }
        close(kind)
        guard url.startAccessingSecurityScopedResource() else { return false }
        folders[kind] = url
        missing.remove(kind)
        return true
    }

    private func close(_ kind: Kind) {
        guard let url = folders.removeValue(forKey: kind) else { return }
        // The other kind may use the same folder: access is counted per call,
        // so stopping ours leaves theirs.
        url.stopAccessingSecurityScopedResource()
    }

    private static func isFolder(_ url: URL) -> Bool {
        var dir: ObjCBool = false
        return FileManager.default.fileExists(atPath: url.path, isDirectory: &dir) && dir.boolValue
    }

    private static func folderPanel(for kind: Kind, current: URL?) -> NSOpenPanel {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = current ?? kind.startDirectory
        return panel
    }
}
